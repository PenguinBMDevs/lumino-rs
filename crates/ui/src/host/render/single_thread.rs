use crate::host::Host;
use iced_wgpu::wgpu;

impl Host {
    /// 单线程渲染模式
    pub(super) fn render_single_thread(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 旧架构：单线程或旧多线程模式
        // 轻量窗口（dialog/progress）没有音符/网格渲染器，直接渲染 UI
        if self.render_ctx.note_renderer.is_none() || self.render_ctx.grid_renderer.is_none() {
            if !self.skip_ui_rendering {
                self.render_iced_ui(frame, view);
            }
            return;
        }

        // 第一步：使用 wgpu 渲染音符和网格（位于 UI 层下方）
        self.render_notes_cached(frame, view, gfx);

        // 第二步：渲染 iced UI（仅在需要时重建 UI 树）
        if !self.skip_ui_rendering {
            self.render_iced_ui(frame, view);
        }
    }

    /// 使用缓存的渲染 - 避免重复上传数据
    pub(super) fn render_notes_cached(
        &mut self,
        _frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        puffin::profile_function!();

        self.update_cursor_for_rendering();

        let clear_color = self.get_clear_color();
        let mut encoder = self.create_render_encoder(gfx);

        let viewport = self.collect_viewport_info();
        let current_viewport_hash = self.compute_current_viewport_hash(&viewport);

        // 准备网格和音符数据
        self.prepare_grid_if_needed(gfx, current_viewport_hash, &viewport);
        let notes_changed = self.prepare_notes_if_needed(current_viewport_hash);

        // 准备相机和深度纹理
        let camera = self.build_camera_uniform(&viewport);
        self.prepare_note_renderer(gfx, &mut encoder, notes_changed, camera);
        self.ensure_depth_texture(gfx, &viewport.physical_size);

        // 执行渲染
        self.execute_render_pass(gfx, encoder, view, clear_color, &viewport);
    }

    /// 更新光标（用于渲染预览音符）
    pub(super) fn update_cursor_for_rendering(&mut self) {
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            self.root
                .update_editor_cursor(self.window_ctx.cursor_position);
        }
    }

    /// 获取清除颜色
    pub(super) fn get_clear_color(&self) -> wgpu::Color {
        let bg_color = self.root.theme().palette().background;
        wgpu::Color {
            r: bg_color.r as f64,
            g: bg_color.g as f64,
            b: bg_color.b as f64,
            a: bg_color.a as f64,
        }
    }

    /// 创建渲染编码器
    pub(super) fn create_render_encoder(&self, gfx: &lumino_gfx::Context) -> wgpu::CommandEncoder {
        gfx.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            })
    }
}
