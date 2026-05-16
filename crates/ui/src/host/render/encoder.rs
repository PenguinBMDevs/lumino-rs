use super::data::{GridColors, ViewportInfo};
use crate::host::Host;
use iced_wgpu::wgpu;

impl Host {
    /// 如果需要则准备网格
    pub(super) fn prepare_grid_if_needed(
        &mut self,
        gfx: &lumino_gfx::Context,
        current_hash: u64,
        viewport: &ViewportInfo,
    ) {
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

        self.render_ctx.grid_renderer.prepare(
            &gfx.queue,
            (viewport.logical_size.width, viewport.logical_size.height),
            v.scroll_x,
            v.scroll_y,
            v.zoom_x,
            v.zoom_y,
            v.keyboard_width,
            v.ruler_height,
            colors.bg,
            colors.black_key,
            colors.bar_line,
            colors.beat_line,
            colors.half_beat_line,
            colors.grid_line,
            colors.key_line,
            v.ppq as f32,
            max_key_index,
            viewport.canvas_offset.x,
            viewport.canvas_offset.y,
        );
    }

    /// 如果需要则准备音符
    pub(super) fn prepare_notes_if_needed(&mut self, current_hash: u64) -> bool {
        let note_index_dirty = self.root.editor.note_index_dirty.get();
        let current_edit_state = self.root.editor.editor_state.interaction.edit_state.clone();
        let is_drawing = matches!(current_edit_state, crate::editor::EditState::Drawing { .. });

        // 视口变化（滚动/缩放）：重新过滤洋葱皮实例（无需全量重建）
        let viewport_changed = current_hash != self.render_ctx.render_cache.note_viewport_hash;

        // 分离 is_drawing：音符数据是否真的变了（排除仅绘制中音符位置变化）
        let note_data_changed =
            note_index_dirty || self.render_ctx.render_cache.note_instances_is_empty();

        if !note_data_changed && !viewport_changed && !is_drawing {
            // 即使没有数据变化也更新状态
            self.render_ctx.last_cursor_position = self.window_ctx.cursor_position;
            self.render_ctx.last_edit_state = current_edit_state;
            return false;
        }

        // 计算可见 tick 范围（CPU 预过滤用）
        let es = &self.root.editor.editor_state;
        let visible_tick_start = (es.view.scroll_x / es.view.zoom_x).max(0.0);
        let visible_tick_end = ((es.view.scroll_x + es.canvas.size.x - es.view.keyboard_width)
            / es.view.zoom_x)
            .max(visible_tick_start);

        // 有变化时才重建实例数组
        if note_data_changed || viewport_changed {
            // 全量重建（包括洋葱皮），CPU 预过滤仅可见区间
            puffin::profile_scope!("generate_note_instances");
            self.update_all_note_instances_fast(visible_tick_start, visible_tick_end);
            self.render_ctx.render_cache.note_viewport_hash = current_hash;
        } else if is_drawing {
            // 仅绘制中音符变化 → 从缓存写双缓冲
            puffin::profile_scope!("generate_note_instances");
            let drawing_note =
                Self::extract_drawing_note(&self.root.editor.editor_state.interaction.edit_state);
            let default_note_length = es.view.default_note_length;
            let snap_precision = es.view.snap_precision;
            self.write_cached_instances_to_buffer(
                drawing_note,
                default_note_length,
                snap_precision,
            );
        }

        self.render_ctx.last_edit_state = current_edit_state;
        self.render_ctx.last_cursor_position = self.window_ctx.cursor_position;

        if note_index_dirty {
            self.root.editor.note_index_dirty.set(false);
            tracing::debug!("Cleared note_index_dirty flag");
        }

        // 数据变化或视口变化都需要 GPU 上传
        note_data_changed || viewport_changed || is_drawing
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

        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }
}
