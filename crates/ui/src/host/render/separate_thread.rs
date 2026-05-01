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
        self.redraw_separate_thread();
        self.copy_offscreen_texture_to_surface(frame, gfx);

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
        let Some(ref wgpu_thread) = self.wgpu_render_thread else {
            return;
        };

        let texture_ref = wgpu_thread
            .latest_texture
            .lock()
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
    pub(super) fn redraw_separate_thread(&mut self) {
        puffin::profile_function!();
        puffin::profile_scope!("redraw_separate_thread");

        if !self.validate_render_thread_ready() {
            return;
        }

        // 准备洋葱皮位图（在单独线程中也需生成位图纹理）
        // 构建视口信息（与encoder.rs中prepare_onion_skin_bitmaps类似）
        let viewport_info = self.collect_viewport_info();
        self.prepare_onion_skin_bitmaps(&viewport_info);

        let render_data = self.collect_render_data();
        let params = self.build_render_params(render_data);

        // 发送渲染参数到 WGPU 线程（非阻塞）
        if let Some(ref wgpu_thread) = self.wgpu_render_thread {
            wgpu_thread.send_params(params);
        }
    }

    /// 验证渲染线程是否就绪
    pub(super) fn validate_render_thread_ready(&self) -> bool {
        if self.wgpu_render_thread.is_none() {
            tracing::error!("redraw_separate_thread called but wgpu_render_thread is None");
            return false;
        }

        if self.note_events_tx.is_none() {
            tracing::error!("redraw_separate_thread called but note_events_tx is None");
            return false;
        }

        true
    }

    /// 收集渲染所需的数据
    pub(super) fn collect_render_data(&mut self) -> super::data::RenderData {
        let editor = &self.root.editor;
        let scroll = editor.scroll();
        let zoom = editor.zoom();
        let viewport_size = self.viewport.logical_size();

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
            self.generate_keyboard_instances(
                super::DEFAULT_KEYBOARD_WIDTH,
                super::DEFAULT_RULER_HEIGHT,
                scroll.1,
                zoom.1,
                super::DEFAULT_VISIBLE_KEY_COUNT,
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

        self.update_note_data_for_wgpu_thread();

        super::data::RenderData {
            scroll,
            zoom,
            viewport_size,
            grid_instances,
            keyboard_instances,
            ruler_instances,
        }
    }

    /// 更新音符数据并发送到 WGPU 线程
    pub(super) fn update_note_data_for_wgpu_thread(&mut self) {
        puffin::profile_scope!("update_note_data");
        let note_index_dirty = self.root.editor.note_index_dirty.get();
        let is_drawing = matches!(
            self.root.editor.edit_state,
            crate::editor::EditState::Drawing { .. }
        );

        let note_data_changed = note_index_dirty
            || unsafe { self.render_cache.note_instances_is_empty() }
            || is_drawing;

        if !note_data_changed {
            return;
        }

        puffin::profile_scope!("update_all_note_instances_fast");
        self.update_all_note_instances_fast();

        // 双缓冲模式下，数据已经通过 swap() 传递，不需要 clone
        // 渲染线程可以直接从双缓冲读取
        if let Some(ref _tx) = self.note_events_tx {
            // 注意：如果使用独立渲染线程，需要通过其他方式同步
            // 这里暂时保留通道发送，但实际数据已经通过双缓冲传递
            // TODO: 重构为直接使用双缓冲读取
        }

        if note_index_dirty {
            self.root.editor.note_index_dirty.set(false);
        }
    }

    /// 构建渲染参数
    pub(super) fn build_render_params(&self, data: super::data::RenderData) -> RenderParams {
        let canvas_offset = self.root.editor.canvas_offset;
        let canvas_size = self.root.editor.canvas_size;
        let physical_size = self.viewport.physical_size();
        let theme = self.root.theme();
        let colors = super::data::GridColors::from_theme(&theme);

        let bg_color = [
            colors.bg[0] as f64,
            colors.bg[1] as f64,
            colors.bg[2] as f64,
            colors.bg[3] as f64,
        ];
        let ppq = self.root.editor.state.ppq;
        let keyboard_width = self.root.editor.state.keyboard_width;
        let ruler_height = self.root.editor.state.ruler_height;
        let max_key_index = (self.root.editor.state.visible_key_count.saturating_sub(1)) as f32;

        // 收集洋葱皮位图视图（活跃音轨中非脏的位图）
        let (onion_skin_bitmap_views, onion_skin_bitmap_sampler) = {
            if self.root.editor.is_onion_skin_enabled() {
                let onion_states = self.root.sidebar.get_onion_skin_states();
                let current_track = self.root.editor.current_track;
                let config = self.root.editor.onion_skin_config();
                let active_tracks: Vec<usize> = onion_states
                    .iter()
                    .filter(|entry| {
                        *entry.1
                            && *entry.0 != current_track
                            && config.should_show_track(*entry.0, true)
                    })
                    .map(|entry| *entry.0)
                    .collect();
                self.onion_skin_bitmaps.collect_views(&active_tracks)
            } else {
                (Vec::new(), None)
            }
        };

        RenderParams {
            viewport_size: (physical_size.width, physical_size.height),
            logical_size: (data.viewport_size.width, data.viewport_size.height),
            scale_factor: self.viewport.scale_factor(),
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
            canvas_offset: (canvas_offset.x, canvas_offset.y),
            canvas_size: (canvas_size.x, canvas_size.y),
            ppq: ppq as f32,
            max_key_index,
            onion_skin_bitmap_views,
            onion_skin_bitmap_sampler,
        }
    }
}
