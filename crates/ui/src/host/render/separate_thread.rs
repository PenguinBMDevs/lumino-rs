use crate::RenderParams;
use crate::host::Host;
use iced_wgpu::wgpu;

impl Host {
    /// 分离渲染线程模式
    pub(super) fn render_with_separate_thread(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        gfx: &lumino_gfx::Context,
    ) {
        let frame_sent = self.redraw_separate_thread();

        // 只有成功发送了渲染参数，才需要复制离屏纹理
        // 如果帧被丢弃（渲染线程忙），跳过复制，避免无意义的 GPU 操作
        if frame_sent {
            self.copy_offscreen_texture_to_surface(frame, gfx);
        }

        // iced UI 仍然需要在主线程渲染到当前 surface
        if !self.skip_ui_rendering {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.render_iced_ui(frame, &view);
        }
    }

    /// 将离屏渲染结果复制到当前 Surface
    pub(super) fn copy_offscreen_texture_to_surface(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        gfx: &lumino_gfx::Context,
    ) {
        let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread else {
            return;
        };

        let texture_ref = wgpu_thread
            .latest_texture
            .read()
            .ok()
            .and_then(|g| g.clone());

        let Some(texture) = texture_ref else {
            return;
        };

        // 确保尺寸匹配，如果因为调整大小等原因不匹配则跳过这帧的复制
        if texture.width() != frame.texture.width() || texture.height() != frame.texture.height() {
            return;
        }

        puffin::profile_scope!("copy_offscreen_texture");
        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy_offscreen_texture_encoder"),
            });

        encoder.copy_texture_to_texture(
            texture.as_image_copy(),
            frame.texture.as_image_copy(),
            wgpu::Extent3d {
                width: texture.width(),
                height: texture.height(),
                depth_or_array_layers: 1,
            },
        );

        gfx.queue.submit(std::iter::once(encoder.finish()));
    }

    /// 分离渲染线程模式的主渲染逻辑
    ///
    /// UI 线程只负责：
    /// 1. 更新状态
    /// 2. 生成渲染参数
    /// 3. 写入音符数据到双缓冲
    /// 4. 发送渲染参数到 WGPU 线程
    ///
    /// 返回 true 表示帧已发送，false 表示被丢弃
    pub(super) fn redraw_separate_thread(&mut self) -> bool {
        puffin::profile_function!();
        puffin::profile_scope!("redraw_separate_thread");

        if !self.validate_render_thread_ready() {
            return false;
        }

        let render_data = self.collect_render_data();
        let params = self.build_render_params(render_data);

        // 发送渲染参数到 WGPU 线程（带背压控制）
        if let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread {
            wgpu_thread.send_params(params)
        } else {
            false
        }
    }

    /// 验证渲染线程是否就绪
    pub(super) fn validate_render_thread_ready(&self) -> bool {
        if self.render_ctx.wgpu_render_thread.is_none() {
            tracing::error!("redraw_separate_thread called but wgpu_render_thread is None");
            return false;
        }

        true
    }

    /// 收集渲染所需的数据
    pub(super) fn collect_render_data(&mut self) -> super::data::RenderData {
        let editor = &self.root.editor;
        let scroll = editor.scroll();
        let zoom = editor.zoom();
        let viewport_size = self.render_ctx.viewport.logical_size();

        // 计算视口哈希，检测网格/键盘/标尺是否需要重新生成
        let v = &editor.editor_state.view;
        let canvas_size = &editor.editor_state.canvas.size;
        let current_hash = crate::host::RenderCache::compute_viewport_hash(
            v.scroll_x,
            v.scroll_y,
            v.zoom_x,
            v.zoom_y,
            canvas_size.x,
            canvas_size.y,
            v.visible_key_count,
        );

        // 检查视口是否变化（短暂借用 cache，不跨越 self 调用）
        let viewport_changed = {
            let cache = &mut self.render_ctx.render_cache;
            let changed = current_hash != cache.separate_thread_viewport_hash;
            if changed {
                cache.separate_thread_viewport_hash = current_hash;
            }
            changed
        };

        if viewport_changed {
            let grid_instances = {
                puffin::profile_scope!("generate_grid_instances");
                self.generate_grid_instances(
                    viewport_size.width,
                    viewport_size.height,
                    super::DEFAULT_KEYBOARD_WIDTH,
                    super::DEFAULT_RULER_HEIGHT,
                    scroll.0,
                    scroll.1,
                    zoom.0,
                    zoom.1,
                )
            };

            let keyboard_instances = {
                puffin::profile_scope!("generate_keyboard_instances");
                let actual_key_count = self.root.editor.editor_state.view.visible_key_count;
                self.generate_keyboard_instances(
                    super::DEFAULT_KEYBOARD_WIDTH,
                    super::DEFAULT_RULER_HEIGHT,
                    scroll.1,
                    zoom.1,
                    actual_key_count,
                )
            };

            let ruler_instances = {
                puffin::profile_scope!("generate_ruler_instances");
                self.generate_ruler_instances(
                    viewport_size.width,
                    super::DEFAULT_KEYBOARD_WIDTH,
                    super::DEFAULT_RULER_HEIGHT,
                    scroll.0,
                    zoom.0,
                    super::TICKS_PER_MEASURE,
                    super::TICKS_PER_BEAT,
                )
            };

            // 更新缓存（短暂借用）
            let cache = &mut self.render_ctx.render_cache;
            cache.grid_instances = grid_instances;
            cache.keyboard_instances = keyboard_instances;
            cache.ruler_instances = ruler_instances;
        }

        self.update_note_data_for_wgpu_thread();

        // 从缓存克隆数据（短暂借用，不跨越 self 调用）
        let (grid_instances, keyboard_instances, ruler_instances) = {
            let cache = &self.render_ctx.render_cache;
            (
                cache.grid_instances.clone(),
                cache.keyboard_instances.clone(),
                cache.ruler_instances.clone(),
            )
        };

        super::data::RenderData {
            scroll,
            zoom,
            viewport_size,
            grid_instances,
            keyboard_instances,
            ruler_instances,
            grid_data_changed: viewport_changed,
            keyboard_data_changed: viewport_changed,
            ruler_data_changed: viewport_changed,
        }
    }

    /// 更新音符数据并发送到 WGPU 线程
    ///
    /// 核心优化（基于火焰图 96-360ms 瓶颈）：
    /// - 音符数据未变 + 仅视口变化 → 跳过全量重建（GPU 已有全量音符数据）
    /// - 音符数据变化 → 全量重建 + SwappableBuffer swap
    pub(super) fn update_note_data_for_wgpu_thread(&mut self) {
        puffin::profile_scope!("update_note_data");
        let note_index_dirty = self.root.editor.note_index_dirty.get();
        let is_drawing = matches!(
            self.root.editor.editor_state.interaction.edit_state,
            crate::editor::EditState::Drawing { .. }
        );

        let note_data_changed = note_index_dirty
            || self.render_ctx.render_cache.note_instances_is_empty()
            || is_drawing;

        // 检测视口变化（滚动/缩放）
        let v = &self.root.editor.editor_state.view;
        let canvas_size = &self.root.editor.editor_state.canvas.size;
        let current_viewport_hash = crate::host::RenderCache::compute_viewport_hash(
            v.scroll_x,
            v.scroll_y,
            v.zoom_x,
            v.zoom_y,
            canvas_size.x,
            canvas_size.y,
            v.visible_key_count,
        );
        let viewport_changed =
            current_viewport_hash != self.render_ctx.render_cache.note_viewport_hash;

        // 什么都没变 → 跳过
        if !note_data_changed && !viewport_changed {
            return;
        }

        // 【关键优化】视口变化 + 音符数据未变 → 免迭代主音轨，只重建洋葱皮
        //
        // 主音轨 NoteInstance 已缓存在 cached_main_note_instances 中（full rebuild 时更新），
        // 此处直接从缓存 clone（~0.2ms / 50k 音符），避免 im::Vector 全量迭代。
        // 洋葱皮实例依赖视口范围，每次视口变化必须重建。
        if viewport_changed && !note_data_changed {
            // 提取所有需要的主音轨引用（避免多个 borrow 冲突）
            let main_instances = self.render_ctx.render_cache.cached_main_note_instances.clone();
            let onion_states = self.root.sidebar.get_onion_skin_states();
            let es_scroll_x = self.root.editor.editor_state.view.scroll_x;
            let es_zoom_x = self.root.editor.editor_state.view.zoom_x;
            let es_canvas_x = self.root.editor.editor_state.canvas.size.x;
            let es_kb_width = self.root.editor.editor_state.view.keyboard_width;
            let es_max_key = self.root.editor.editor_state.view.visible_key_count.saturating_sub(1);

            // 计算洋葱皮视口范围
            let visible_tick_start = (es_scroll_x / es_zoom_x).max(0.0);
            let visible_tick_end = ((es_scroll_x + es_canvas_x - es_kb_width) / es_zoom_x)
                .max(visible_tick_start);

            // 获取洋葱皮实例（需要可变借用 self.root.editor）
            let onion_instances = self.root.editor.get_all_onion_skin_instances_in_range(
                &onion_states,
                visible_tick_start,
                visible_tick_end,
                0u16,
                es_max_key,
            );

            // 获取编辑状态（绘制中音符）
            let edit_state = &self.root.editor.editor_state.interaction.edit_state;
            let default_note_length = self.root.editor.editor_state.view.default_note_length;
            let snap_precision = self.root.editor.editor_state.view.snap_precision;

            // 写入双缓冲：缓存主音轨 + 新洋葱皮 + 绘制中音符
            let instances = unsafe {
                self.render_ctx
                    .render_cache
                    .note_instances_buffer
                    .write_buffer()
            };
            instances.clear();
            instances.reserve(main_instances.len() + onion_instances.len() + 1);
            instances.extend(main_instances);
            instances.extend(onion_instances);
            Self::add_drawing_note_to_instances(
                instances,
                edit_state,
                default_note_length,
                snap_precision,
            );

            self.render_ctx.render_cache.note_viewport_hash = current_viewport_hash;
            self.render_ctx.render_cache.note_instances_version =
                self.render_ctx.render_cache.note_instances_buffer.swap();
            return;
        }

        // 音符数据变化 → 全量重建
        puffin::profile_scope!("update_all_note_instances_fast");
        self.update_all_note_instances_fast();
        self.render_ctx.render_cache.note_viewport_hash = current_viewport_hash;

        if note_index_dirty {
            self.root.editor.note_index_dirty.set(false);
        }
    }

    /// 构建渲染参数
    pub(super) fn build_render_params(&self, data: super::data::RenderData) -> RenderParams {
        let es = &self.root.editor.editor_state;
        let canvas_offset = es.canvas.offset;
        let canvas_size = es.canvas.size;
        let physical_size = self.render_ctx.viewport.physical_size();
        let theme = self.root.theme();
        let colors = super::data::GridColors::from_theme(&theme);

        let bg_color = [
            colors.bg[0] as f64,
            colors.bg[1] as f64,
            colors.bg[2] as f64,
            colors.bg[3] as f64,
        ];
        let ppq = es.view.ppq;
        let keyboard_width = es.view.keyboard_width;
        let ruler_height = es.view.ruler_height;
        let max_key_index = (es.view.visible_key_count.saturating_sub(1)) as f32;

        RenderParams {
            viewport_size: (physical_size.width, physical_size.height),
            logical_size: (data.viewport_size.width, data.viewport_size.height),
            scale_factor: self.render_ctx.viewport.scale_factor(),
            scroll: data.scroll,
            zoom: data.zoom,
            keyboard_width,
            ruler_height,
            background_color: bg_color,
            color_bg: colors.bg,
            color_bg_black_key: colors.black_key,
            color_bar: colors.bar_line,
            color_beat: colors.beat_line,
            color_half_beat: colors.half_beat_line,
            color_grid: colors.grid_line,
            color_key_line: colors.key_line,
            grid_instances: data.grid_instances,
            note_instances: vec![],
            ruler_instances: data.ruler_instances,
            keyboard_instances: data.keyboard_instances,
            ticks_per_measure: (ppq as u32) * 4,
            ticks_per_beat: ppq as u32,
            regenerate_grid: false,
            grid_data_changed: data.grid_data_changed,
            keyboard_data_changed: data.keyboard_data_changed,
            ruler_data_changed: data.ruler_data_changed,
            canvas_offset: (canvas_offset.x, canvas_offset.y),
            canvas_size: (canvas_size.x, canvas_size.y),
            ppq: ppq as f32,
            max_key_index,
        }
    }
}
