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
        let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread else {
            return;
        };

        let texture_ref = wgpu_thread
            .latest_texture
            .try_lock()
            .ok()
            .and_then(|g| g.clone());

        let Some(texture) = texture_ref else {
            return;
        };

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

        let render_data = self.collect_render_data();
        let params = self.build_render_params(render_data);

        // 发送渲染参数到 WGPU 线程（非阻塞）
        if let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread {
            wgpu_thread.send_params(params);
        }
    }

    /// 验证渲染线程是否就绪
    pub(super) fn validate_render_thread_ready(&self) -> bool {
        if self.render_ctx.wgpu_render_thread.is_none() {
            tracing::error!("redraw_separate_thread called but wgpu_render_thread is None");
            return false;
        }

        if self.render_ctx.note_events_tx.is_none() {
            tracing::error!("redraw_separate_thread called but note_events_tx is None");
            return false;
        }

        true
    }

    /// 收集渲染所需的数据
    pub(super) fn collect_render_data(&mut self) -> super::data::RenderData {
        let viewport_size = self.render_ctx.viewport.logical_size();

        let (scroll, zoom) = if self.root.is_arrangement_mode() {
            let av = &self.root.arrangement_view.viewport;
            // yinhe 风格：y 坐标使用像素值，zoom_y = 1.0
            ((av.scroll_x, av.scroll_y), (av.zoom_x, 1.0))
        } else {
            let editor = &self.root.editor;
            (editor.scroll(), editor.zoom())
        };

        let grid_instances = if self.root.is_arrangement_mode() {
            vec![] // 音轨总览模式下跳过网格
        } else {
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

        // WGPU 键盘渲染已移除，使用 Iced Canvas 键盘替代
        let keyboard_instances = vec![];

        let ruler_instances = if self.root.is_arrangement_mode() {
            vec![] // 音轨总览模式下跳过标尺
        } else {
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

    /// 更新音符数据：主音符同步写入 + 洋葱皮异步派发
    ///
    /// Phase 1: 主音轨主音符 → 主线程同步写入双缓冲 + swap（~1ms）
    ///   → WGPU 线程立即可见，零延迟，白屏问题根治
    /// Phase 2: 洋葱皮 → 派发到 NoteWorker 异步计算，完成后二次 swap
    ///   → 50-200ms 延迟，但不阻塞主音符渲染
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

        // 检测视口变化（滚动/缩放）：音轨总览模式使用 arrangement viewport
        let (current_viewport_hash, viewport_changed) = if self.root.is_arrangement_mode() {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as u16;
            let hash = crate::host::RenderCache::compute_viewport_hash(
                av.scroll_x,
                av.scroll_y,
                av.zoom_x,
                av.track_height,
                av.canvas_size.x,
                av.canvas_size.y,
                track_count,
            );
            let changed = hash != self.render_ctx.render_cache.note_viewport_hash;
            (hash, changed)
        } else {
            let v = &self.root.editor.editor_state.view;
            let canvas_size = &self.root.editor.editor_state.canvas.size;
            let hash = crate::host::RenderCache::compute_viewport_hash(
                v.scroll_x,
                v.scroll_y,
                v.zoom_x,
                v.zoom_y,
                canvas_size.x,
                canvas_size.y,
                v.visible_key_count,
            );
            let changed = hash != self.render_ctx.render_cache.note_viewport_hash;
            (hash, changed)
        };

        if !note_data_changed && !viewport_changed {
            return;
        }

        self.render_ctx.render_cache.note_viewport_hash = current_viewport_hash;

        // ═══ Phase 1: 主音符同步写入（保证 WGPU 立即可见） ═══
        {
            puffin::profile_scope!("phase1_main_notes_sync");
            // 工程走带模式：音符由 iced Canvas 直接绘制，不经过 WGPU NoteRenderer
            // 所以跳过 note_instances_buffer 的写入
            if !self.root.is_arrangement_mode() {
                // 钢琴卷帘模式：只生成当前音轨的音符
                let notes_clone = self.root.editor.editor_state.data.notes.clone(); // O(1)
                let edit_state_clone = self.root.editor.editor_state.interaction.edit_state.clone();
                let default_note_length = self.root.editor.editor_state.view.default_note_length;
                let snap_precision = self.root.editor.editor_state.view.snap_precision;
                super::note_worker::build_main_note_instances(
                    &self.render_ctx.render_cache.note_instances_buffer,
                    &notes_clone,
                    &edit_state_clone,
                    default_note_length,
                    snap_precision,
                );
            }
        }

        // ═══ Phase 2: 洋葱皮异步派发（独立 buffer，fire-and-forget） ═══
        self.ensure_note_worker();
        if let Some(ref worker) = self.render_ctx.note_worker {
            puffin::profile_scope!("dispatch_onion_skin_job");
            let vp_logical = self.render_ctx.viewport.logical_size();
            let os_snapshot =
                self.collect_onion_skin_snapshot((vp_logical.width, vp_logical.height));

            worker.send(super::note_worker::OnionSkinJob {
                snapshot: os_snapshot,
                onion_note_buffer: std::sync::Arc::clone(
                    &self.render_ctx.render_cache.onion_note_buffer,
                ),
                done_tx: None,
            });
        }

        if note_index_dirty {
            self.root.editor.note_index_dirty.set(false);
        }
    }

    /// 构建渲染参数
    pub(super) fn build_render_params(&self, data: super::data::RenderData) -> RenderParams {
        let es = &self.root.editor.editor_state;
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
        let is_arrangement_mode = self.root.is_arrangement_mode();
        let max_key_index = if is_arrangement_mode {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as f32;
            track_count * av.track_height - av.track_height / 128.0
        } else {
            (es.view.visible_key_count.saturating_sub(1)) as f32
        };

        let (canvas_offset, canvas_size, keyboard_width, ruler_height) = if is_arrangement_mode {
            let av = &self.root.arrangement_view.viewport;
            (
                (av.canvas_offset.x, av.canvas_offset.y),
                (av.canvas_size.x, av.canvas_size.y),
                0.0,
                0.0,
            )
        } else {
            (
                (es.canvas.offset.x, es.canvas.offset.y),
                (es.canvas.size.x, es.canvas.size.y),
                es.view.keyboard_width,
                es.view.ruler_height,
            )
        };

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
            canvas_offset: (canvas_offset.0, canvas_offset.1),
            canvas_size: (canvas_size.0, canvas_size.1),
            ppq: ppq as f32,
            max_key_index,
            is_arrangement_mode,
        }
    }
}
