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
        if current_hash == self.render_cache.grid_viewport_hash {
            return;
        }

        // 视口变化，重新生成网格线
        self.root
            .update_grid_line_instances(&mut self.render_cache.grid_instances);
        self.render_cache.grid_viewport_hash = current_hash;

        let theme = self.root.theme();
        let colors = GridColors::from_theme(&theme);
        let editor = &self.root.editor;
        let max_key_index = (editor.state.visible_key_count.saturating_sub(1)) as f32;

        self.grid_renderer.prepare(
            &[],
            &gfx.device,
            &gfx.queue,
            (viewport.logical_size.width, viewport.logical_size.height),
            editor.state.scroll_x,
            editor.state.scroll_y,
            editor.state.zoom_x,
            editor.state.zoom_y,
            editor.state.keyboard_width,
            editor.state.ruler_height,
            colors.bg,
            colors.black_key,
            colors.bar_line,
            colors.beat_line,
            colors.half_beat_line,
            colors.grid_line,
            colors.key_line,
            editor.state.ppq as f32,
            max_key_index,
            viewport.canvas_offset.x,
            viewport.canvas_offset.y,
        );
    }

    /// 如果需要则准备音符
    pub(super) fn prepare_notes_if_needed(&mut self, current_hash: u64) -> bool {
        let note_index_dirty = self.root.editor.note_index_dirty.get();
        let current_edit_state = self.root.editor.edit_state.clone();
        let is_drawing = matches!(current_edit_state, crate::editor::EditState::Drawing { .. });

        let note_data_changed =
            note_index_dirty || self.render_cache.note_instances.is_empty() || is_drawing;

        if !note_data_changed {
            // 即使没有数据变化也更新状态
            self.last_cursor_position = self.cursor_position;
            self.last_edit_state = current_edit_state;
            return false;
        }

        puffin::profile_scope!("generate_note_instances");
        self.update_all_note_instances_fast();
        self.render_cache.note_viewport_hash = current_hash;
        self.last_edit_state = current_edit_state;
        self.last_cursor_position = self.cursor_position;

        if note_index_dirty {
            self.root.editor.note_index_dirty.set(false);
            tracing::debug!("Cleared note_index_dirty flag");
        }

        true
    }

    /// 准备音符渲染器
    pub(super) fn prepare_note_renderer(
        &mut self,
        gfx: &lumino_gfx::Context,
        encoder: &mut wgpu::CommandEncoder,
        notes_changed: bool,
        camera: lumino_gfx::CameraUniform,
    ) {
        if notes_changed && !self.render_cache.note_instances.is_empty() {
            self.note_renderer.prepare_old(
                encoder,
                &self.render_cache.note_instances,
                &gfx.device,
                &gfx.queue,
                camera,
            );
        } else if !self.render_cache.note_instances.is_empty() {
            self.note_renderer.prepare_pass(encoder, camera, &gfx.queue);
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

        self.render_cache.depth_texture = Some((
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
        let Some((_, _, depth_view)) = self.render_cache.depth_texture.as_ref() else {
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
            self.grid_renderer.draw(&mut render_pass, 1);
        }

        // 绘制音符
        if !self.render_cache.note_instances.is_empty() && scissor.has_valid_region {
            render_pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
            self.note_renderer.draw(
                &mut render_pass,
                true,
                Some((scissor.x, scissor.y, scissor.width, scissor.height)),
            );
        }

        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }
}
