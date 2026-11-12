use crate::host::Host;
use iced_core::{Color, Event, renderer, window as iced_window};
use iced_wgpu::wgpu;
use iced_winit::runtime::user_interface::{self, UserInterface};

/// egui 是否独占整窗渲染（生产路径）。
///
/// 默认开启：外壳 / 对话框 / 面板 / 编辑器画布底层全部由 egui 绘制，
/// iced 只保留事件与测量代码。`LUMINO_EGUI_ONLY=0` 回退到「iced present +
/// egui 叠加」的旧通道，用于对照排查。
pub(crate) fn egui_only_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("LUMINO_EGUI_ONLY")
            .map(|value| value != "0")
            .unwrap_or(true)
    })
}

/// 主题色（egui 色板 `base_bg`）→ wgpu 清屏色（sRGB）。
fn clear_color(palette: lumino_ui_egui::theme::Palette) -> wgpu::Color {
    let base = palette.base_bg;
    let srgb = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    wgpu::Color {
        r: srgb(base.r()),
        g: srgb(base.g()),
        b: srgb(base.b()),
        a: 1.0,
    }
}

impl Host {
    /// 渲染窗口画面。
    ///
    /// `background` 为 `Some(color)` 时以该颜色覆盖整窗（无编辑器画布的窗口模式）；
    /// 为 `None` 时由 egui 采样 gfx 离屏纹理作底层。
    pub(super) fn render_iced_ui(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        texture_view: &wgpu::TextureView,
        background: Option<Color>,
    ) {
        puffin::profile_function!();

        // 生产路径：egui 独占整窗，不再构建 iced widget 树、不再 `present`。
        // 底层（gfx 离屏纹理）由 egui 以外部纹理采样后整窗绘制，其余 UI 全在 egui。
        if egui_only_enabled() && crate::host::overlay_enabled() {
            let clear = match background {
                Some(color) => wgpu::Color {
                    r: f64::from(color.r),
                    g: f64::from(color.g),
                    b: f64::from(color.b),
                    a: 1.0,
                },
                None => clear_color(self.root.egui_palette()),
            };
            self.render_egui_overlay(frame, texture_view, Some(clear));
            return;
        }

        // 如果 UI 没有变更，跳过 UI 重建和绘制
        // 使用实例字段来确保至少渲染一次 UI，避免线程不安全的 static mut
        let is_first_render = !self.render_ctx.has_rendered_ui;

        // 菜单打开时，不使用缓存机制，每次都重建 UI 以避免菜单闪烁
        let is_menu_open = {
            puffin::profile_scope!("check_menu_open");
            !self.root.should_render_preview_note()
        };

        if !is_menu_open && !self.ui_dirty && !is_first_render {
            // UI 没有变化且不是第一次渲染，直接 present 之前渲染的内容
            self.render_ctx.renderer.present(
                background,
                frame.texture.format(),
                texture_view,
                &self.render_ctx.viewport,
            );
            // 叠加层必须在 present 之后重画，否则会被缓存内容覆盖。
            self.render_egui_overlay(frame, texture_view, None);
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

        self.render_ctx.renderer.present(
            background,
            frame.texture.format(),
            texture_view,
            &self.render_ctx.viewport,
        );
        self.render_egui_overlay(frame, texture_view, None);

        // 处理消息（在 interface 被释放之后，避免借用冲突）
        let mut has_state_change = false;
        {
            puffin::profile_scope!("process_iced_messages");
            for message in messages {
                if self.process_message(message) {
                    has_state_change = true;
                }
            }
        }

        // 如果状态有变更或 UI 有更新（如打开下拉菜单），标记 UI 需要重绘并请求下一帧。
        // 仅在主窗口（note_renderer.is_some()）中请求自触发重绘，以维持动画/播放头刷新。
        // 对话框（note_renderer.is_none()）的刷新由 DialogManager::update() 驱动，
        // 其 redraw_force 已确保 view() 重新构建；此处的 request_redraw 会形成
        // RedrawRequested → handle_dialog_event → dialog.redraw() 的无用自循环。
        {
            puffin::profile_scope!("update_ui_state");
            // REND-019（重连后自旋修复）：仅 `has_state_change` 强制下一帧；
            // Updated 的重绘需求以 iced 的 `redraw_request` 为准——
            // `NextFrame` / `At` 由 handle_redraw_request 驱动（含 ui_dirty 置位），
            // 纯 `Wait` 的 Updated **不得**自触发重绘。
            // 背景（实测 2026-10-17）：重连后 iced 每帧返回 `Updated{Wait}`，
            // 旧条件 `(has_state_change || is_ui_updated)` 形成 ~1100fps 全量
            // UI 重建 + 重绘风暴（主线程 100%、R2「CPU 非空转」准出项失守）。
            if has_state_change && self.render_ctx.note_renderer.is_some() {
                self.ui_dirty = true;
                self.window_ctx.window.request_redraw();
            }
        }

        // 更新鼠标光标（幂等，仅变化时真正设置，避免与事件路径竞态闪烁），
        // 并处理 iced 的重绘请求：仅 `State::Updated` 携带有效请求——Tooltip 延迟
        // 显示依赖 `RedrawRequest::At` 的定时唤醒，`Outdated` 无请求可处理。
        {
            puffin::profile_scope!("update_cursor");
            if let user_interface::State::Updated {
                mouse_interaction,
                redraw_request,
                ..
            } = &state
            {
                self.apply_cursor_interaction(*mouse_interaction);
                self.handle_redraw_request(*redraw_request);
            }
        }
    }
}
