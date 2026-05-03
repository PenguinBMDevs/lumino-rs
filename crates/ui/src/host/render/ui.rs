use crate::host::Host;
use iced_core::{Event, renderer, window as iced_window};
use iced_wgpu::wgpu;
use iced_winit::runtime::user_interface::{self, UserInterface};

impl Host {
    /// 渲染 iced UI 层
    pub(super) fn render_iced_ui(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        texture_view: &wgpu::TextureView,
    ) {
        puffin::profile_function!();

        // 如果 UI 没有变更，跳过 UI 重建和绘制
        // 使用实例字段来确保至少渲染一次 UI，避免线程不安全的 static mut
        let is_first_render = !self.render_ctx.has_rendered_ui;

        // 菜单打开时，不使用缓存机制，每次都重建 UI 以避免菜单闪烁
        let is_menu_open = !self.root.should_render_preview_note();

        // 检测光标位置是否变化，如果变化需要更新鼠标指针样式
        let cursor_changed = self.render_ctx.last_render_cursor != self.window_ctx.cursor;

        if !is_menu_open && !self.ui_dirty && !is_first_render && !cursor_changed {
            // UI 没有变化且不是第一次渲染且光标未移动，直接 present 之前渲染的内容
            self.render_ctx.renderer
                .present(None, frame.texture.format(), texture_view, &self.render_ctx.viewport);
            return;
        }

        // 临时取出缓存以避免借用冲突
        let cache = std::mem::take(&mut self.render_ctx.cache);

        let mut interface = {
            puffin::profile_scope!("build_interface");
            UserInterface::build(
                self.root.view(),
                self.render_ctx.viewport.logical_size(),
                cache,
                &mut self.render_ctx.renderer,
            )
        };

        let mut messages = Vec::new();
        let (state, _) = {
            puffin::profile_scope!("update_interface");

            interface.update(
                &[Event::Window(iced_window::Event::RedrawRequested(
                    std::time::Instant::now(),
                ))],
                self.window_ctx.cursor,
                &mut self.render_ctx.renderer,
                &mut self.window_ctx.clipboard,
                &mut messages,
            )
        };

        // 绘制界面
        {
            puffin::profile_scope!("draw_interface");
            let theme = self.root.theme();
            interface.draw(
                &mut self.render_ctx.renderer,
                &theme,
                &renderer::Style::default(),
                self.window_ctx.cursor,
            );
        }

        // 归还缓存
        self.render_ctx.cache = interface.into_cache();
        // 重绘完成后 UI 不再 dirty
        self.ui_dirty = false;

        // 标记已完成首次渲染
        if !self.render_ctx.has_rendered_ui {
            self.render_ctx.has_rendered_ui = true;
        }

        // 更新上次渲染时的光标状态
        self.render_ctx.last_render_cursor = self.window_ctx.cursor;

        self.render_ctx.renderer
            .present(None, frame.texture.format(), texture_view, &self.render_ctx.viewport);

        // 处理消息（在 interface 被释放之后，避免借用冲突）
        let mut has_state_change = false;
        for message in messages {
            if self.process_message(message) {
                has_state_change = true;
            }
        }

        // 如果状态有变更或 UI 有更新（如打开下拉菜单），标记 UI 需要重绘并请求下一帧
        let is_ui_updated = matches!(state, user_interface::State::Updated { .. });
        if has_state_change || is_ui_updated {
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }

        // 更新鼠标光标
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            if let Some(icon) = iced_winit::conversion::mouse_interaction(mouse_interaction) {
                self.window_ctx.window.set_cursor(icon);
                self.window_ctx.window.set_cursor_visible(true);
            } else {
                self.window_ctx.window.set_cursor_visible(false);
            }
        }
    }
}
