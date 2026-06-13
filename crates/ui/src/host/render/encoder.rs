use super::data::{GridColors, ViewportInfo};
use crate::host::Host;
use iced_wgpu::wgpu;
use lumino_gfx::GridPrepareParams;

impl Host {
    /// 如果需要则准备网格
    pub(super) fn prepare_grid_if_needed(
        &mut self,
        gfx: &lumino_gfx::Context,
        current_hash: u64,
        viewport: &ViewportInfo,
    ) {
        if self.root.is_arrangement_mode() {
            // 音轨总览模式下跳过网格准备
            self.render_ctx.render_cache.grid_viewport_hash = current_hash;
            return;
        }

        if current_hash == self.render_ctx.render_cache.grid_viewport_hash {
            return;
        }

        // 视口变化，重新生成网格线
        self.root
            .update_grid_line_instances(&mut self.render_ctx.render_cache.grid_instances);
        self.render_ctx.render_cache.grid_viewport_hash = current_hash;

        let theme = self.root.theme();
        let colors = GridColors::from_theme(&theme);
        let editor = &self.root.editor;
        let v = &editor.editor_state.view;
        let max_key_index = (v.visible_key_count.saturating_sub(1)) as f32;

        let grid_params = GridPrepareParams {
            viewport_size: (viewport.logical_size.width, viewport.logical_size.height),
            scroll_x: v.scroll_x,
            scroll_y: v.scroll_y,
            zoom_x: v.zoom_x,
            zoom_y: v.zoom_y,
            keyboard_width: v.keyboard_width,
            ruler_height: v.ruler_height,
            color_bg: colors.bg,
            color_bg_black_key: colors.black_key,
            color_bar: colors.bar_line,
            color_beat: colors.beat_line,
            color_half_beat: colors.half_beat_line,
            color_grid: colors.grid_line,
            color_key_line: colors.key_line,
            ppq: v.ppq as f32,
            max_key_index,
            canvas_offset_x: viewport.canvas_offset.x,
            canvas_offset_y: viewport.canvas_offset.y,
        };
        self.render_ctx
            .grid_renderer
            .prepare(&gfx.queue, &grid_params);
    }

    /// 如果需要则准备音符
    ///
    /// Phase 1: 主音符同步写入双缓冲 + swap（~1ms，保证 WGPU 立即可见）
    /// Phase 2: 洋葱皮派发到 NoteWorker + 等待完成（done_tx 同步屏障）
    pub(super) fn prepare_notes_if_needed(&mut self, current_hash: u64) -> bool {
        // 视口变化（滚动/缩放）：重新过滤洋葱皮实例（无需全量重建）
        let viewport_changed = current_hash != self.render_ctx.render_cache.note_viewport_hash;

        // 工程走带模式：音符由 iced Canvas 绘制，跳过 WGPU 音符准备
        if self.root.is_arrangement_mode() {
            self.render_ctx.render_cache.note_viewport_hash = current_hash;
            return false;
        }

        let note_index_dirty = self.root.editor.spatial.note_index_dirty.get();
        let current_edit_state = self.root.editor.editor_state.interaction.edit_state.clone();
        let is_drawing = matches!(current_edit_state, crate::editor::EditState::Drawing { .. });

        // 数据变化（编辑/加载）
        let note_data_changed = note_index_dirty
            || self.render_ctx.render_cache.note_instances_is_empty()
            || is_drawing;

        if !note_data_changed && !viewport_changed {
            // 即使没有数据变化也更新状态
            self.render_ctx.last_cursor_position = self.window_ctx.cursor_position;
            self.render_ctx.last_edit_state = current_edit_state;
            return false;
        }

        // 更新视口哈希
        self.render_ctx.render_cache.note_viewport_hash = current_hash;

        // 提取所需数据，避免 &self 借用冲突
        let notes_clone = self.root.editor.editor_state.data.notes.clone(); // O(1)
        let edit_state_clone = self.root.editor.editor_state.interaction.edit_state.clone();
        let default_note_length = self.root.editor.editor_state.view.default_note_length;
        let snap_precision = self.root.editor.editor_state.view.snap_precision;

        // ═══ Phase 1: 主音符同步写入（保证 WGPU 立即可见） ═══
        {
            puffin::profile_scope!("phase1_main_notes_sync");
            super::note_worker::build_main_note_instances(
                &self.render_ctx.render_cache.note_instances_buffer,
                &notes_clone,
                &edit_state_clone,
                default_note_length,
                snap_precision,
            );
        }

        // ═══ Phase 2: 洋葱皮异步派发（独立 buffer，不碰主音符） ═══
        self.ensure_note_worker();
        if let Some(ref worker) = self.render_ctx.note_worker {
            let vp_logical = self.render_ctx.viewport.logical_size();
            let os_snapshot =
                self.collect_onion_skin_snapshot((vp_logical.width, vp_logical.height));
            let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();

            worker.send(super::note_worker::OnionSkinJob {
                snapshot: os_snapshot,
                onion_note_buffer: std::sync::Arc::clone(
                    &self.render_ctx.render_cache.onion_note_buffer,
                ),
                done_tx: Some(done_tx),
            });

            // 单线程模式：等待洋葱皮完成后才能开始渲染
            //（分离渲染模式不需要等待，fire-and-forget）
            let _ = done_rx.recv();
        } else {
            tracing::warn!("prepare_notes_if_needed: No NoteWorker available");
        }

        self.render_ctx.last_edit_state = current_edit_state;
        self.render_ctx.last_cursor_position = self.window_ctx.cursor_position;

        if note_index_dirty {
            self.root.editor.spatial.note_index_dirty.set(false);
            tracing::debug!("Cleared note_index_dirty flag");
        }

        // 数据变化或视口变化都需要 GPU 上传
        note_data_changed || viewport_changed
    }

    /// 准备音符渲染器（双缓冲模式）
    pub(super) fn prepare_note_renderer(
        &mut self,
        gfx: &lumino_gfx::Context,
        encoder: &mut wgpu::CommandEncoder,
        notes_changed: bool,
        camera: lumino_gfx::CameraUniform,
    ) {
        // 从双缓冲的前缓冲区读取音符实例
        let note_instances = unsafe {
            self.render_ctx
                .render_cache
                .note_instances_buffer
                .read_buffer()
        };

        if notes_changed && !note_instances.is_empty() {
            self.render_ctx.note_renderer.prepare_notes(
                encoder,
                note_instances,
                &gfx.device,
                &gfx.queue,
                camera,
            );
        } else if !note_instances.is_empty() {
            self.render_ctx
                .note_renderer
                .prepare_pass(encoder, camera, &gfx.queue);
        }
    }

    /// 确保深度纹理存在
    pub(super) fn ensure_depth_texture(
        &mut self,
        gfx: &lumino_gfx::Context,
        physical_size: &(u32, u32),
    ) {
        let (width, height) = *physical_size;
        let needs_resize = self
            .render_ctx
            .render_cache
            .depth_texture
            .as_ref()
            .is_none_or(|(w, h, _)| *w != width || *h != height);

        if !needs_resize {
            return;
        }

        let depth_tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("host_depth_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        self.render_ctx.render_cache.depth_texture = Some((
            width,
            height,
            depth_tex.create_view(&wgpu::TextureViewDescriptor::default()),
        ));
    }

    /// 执行渲染通道
    pub(super) fn execute_render_pass(
        &mut self,
        gfx: &lumino_gfx::Context,
        mut encoder: wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        clear_color: wgpu::Color,
        viewport: &ViewportInfo,
    ) {
        let Some((_, _, depth_view)) = self.render_ctx.render_cache.depth_texture.as_ref() else {
            tracing::error!("depth_texture not available");
            return;
        };

        let scissor = self.calculate_scissor_rect(viewport);

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // 工程走带模式下跳过 WGPU 绘制（由 iced Canvas 直接渲染）
        if !self.root.is_arrangement_mode() {
            // 绘制网格线
            if scissor.has_valid_region {
                render_pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
                self.render_ctx.grid_renderer.draw(&mut render_pass, 1);
            }

            // 绘制音符（从双缓冲读取）
            let note_instances = unsafe {
                self.render_ctx
                    .render_cache
                    .note_instances_buffer
                    .read_buffer()
            };
            if !note_instances.is_empty() && scissor.has_valid_region {
                render_pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
                self.render_ctx.note_renderer.draw(
                    &mut render_pass,
                    true,
                    Some((scissor.x, scissor.y, scissor.width, scissor.height)),
                );
            }
        }

        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }
}
