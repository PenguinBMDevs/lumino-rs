//! 窗口上下文 —— 从 Host 拆出的窗口/输入相关字段
//!
//! `Host` 不再持有具体窗口类型：只依赖 [`HostWindow`] 抽象（重绘 / 光标 /
//! 缩放 / 最大化），具体窗口由运行时注入。这样 `Host` 可以在无窗口环境
//! （离屏测试、其他后端）中构造，窗口生命周期归运行时所有。

use std::sync::Arc;

use lumino_ui_core::geom::mouse;
use winit;

use crate::window;

/// `Host` 需要的窗口能力。
///
/// 只暴露 UI 层真正用到的五个动作。winit 的 `Window` 直接实现该 trait，
/// 运行时也可以注入自己的适配器（无窗口测试 / 其他后端）。
pub trait HostWindow: Send + Sync {
    /// 请求下一帧重绘。
    fn request_redraw(&self);
    /// 设置系统光标图标。
    fn set_cursor(&self, icon: winit::window::CursorIcon);
    /// 显示 / 隐藏系统光标。
    fn set_cursor_visible(&self, visible: bool);
    /// 当前 DPI 缩放因子（物理像素 / 逻辑像素）。
    fn scale_factor(&self) -> f64;
    /// 窗口是否最大化。
    fn is_maximized(&self) -> bool;
    /// 允许 / 禁止系统输入法（egui 文本框获得焦点时开启）。
    fn set_ime_allowed(&self, allowed: bool);
    /// 把输入法候选窗锚到光标处（物理像素）。
    fn set_ime_cursor_area(
        &self,
        position: winit::dpi::PhysicalPosition<i32>,
        size: winit::dpi::PhysicalSize<u32>,
    );
}

/// winit 窗口的默认实现：直接转发到系统窗口。
impl HostWindow for winit::window::Window {
    fn request_redraw(&self) {
        winit::window::Window::request_redraw(self);
    }

    fn set_cursor(&self, icon: winit::window::CursorIcon) {
        winit::window::Window::set_cursor(self, icon);
    }

    fn set_cursor_visible(&self, visible: bool) {
        winit::window::Window::set_cursor_visible(self, visible);
    }

    fn scale_factor(&self) -> f64 {
        winit::window::Window::scale_factor(self)
    }

    fn is_maximized(&self) -> bool {
        winit::window::Window::is_maximized(self)
    }

    fn set_ime_allowed(&self, allowed: bool) {
        winit::window::Window::set_ime_allowed(self, allowed);
    }

    fn set_ime_cursor_area(
        &self,
        position: winit::dpi::PhysicalPosition<i32>,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        winit::window::Window::set_ime_cursor_area(self, position, size);
    }
}

/// 窗口上下文，持有窗口和输入状态。
pub(crate) struct WindowContext {
    /// 窗口句柄（只经 [`HostWindow`] 抽象访问，不绑定 winit 具体类型）
    pub window: Arc<dyn HostWindow>,
    /// 光标状态
    pub cursor: mouse::Cursor,
    /// 逻辑光标位置
    pub cursor_position: Option<lumino_ui_core::geom::Point>,
    /// 待处理的窗口动作
    pub pending_window_action: Option<window::TrafficAction>,
    /// 是否正在拖拽
    pub pending_drag: bool,
    /// 工具栏拖拽调整标识
    pub is_toolbar_resizing: bool,
    /// 鼠标按钮按下标识
    pub is_mouse_pressed: bool,
    /// 系统输入法是否已开启（只在状态变化时调用 `set_ime_allowed`）。
    pub ime_allowed: bool,
    /// 已实际应用到窗口的光标状态（`None` = 尚未应用；`Some(None)` = 已隐藏）。
    ///
    /// 只在光标图标/可见性**真正变化**时才调用 `set_cursor` / `set_cursor_visible`：
    /// 每帧重复设置会被系统（Windows `WM_SETCURSOR`）与异步设置竞态重置，
    /// 表现为光标在两种形态间闪烁。
    pub applied_cursor: Option<Option<winit::window::CursorIcon>>,
}

impl WindowContext {
    /// 创建窗口上下文
    pub fn new(window: Arc<dyn HostWindow>) -> Self {
        Self {
            window,
            cursor: mouse::Cursor::Unavailable,
            cursor_position: None,
            pending_window_action: None,
            pending_drag: false,
            is_toolbar_resizing: false,
            is_mouse_pressed: false,
            ime_allowed: false,
            applied_cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HostWindow, WindowContext};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    /// 无窗口环境下的假窗口：证明 `Host` 的窗口依赖是可注入的。
    #[derive(Default)]
    struct MockWindow {
        redraws: AtomicUsize,
        cursor_visible: AtomicBool,
        ime_allowed: AtomicBool,
    }

    impl HostWindow for MockWindow {
        fn request_redraw(&self) {
            self.redraws.fetch_add(1, Ordering::SeqCst);
        }

        fn set_cursor(&self, _icon: winit::window::CursorIcon) {}

        fn set_cursor_visible(&self, visible: bool) {
            self.cursor_visible.store(visible, Ordering::SeqCst);
        }

        fn scale_factor(&self) -> f64 {
            1.5
        }

        fn is_maximized(&self) -> bool {
            true
        }

        fn set_ime_allowed(&self, allowed: bool) {
            self.ime_allowed.store(allowed, Ordering::SeqCst);
        }

        fn set_ime_cursor_area(
            &self,
            _position: winit::dpi::PhysicalPosition<i32>,
            _size: winit::dpi::PhysicalSize<u32>,
        ) {
        }
    }

    #[test]
    fn window_context_accepts_injected_window() {
        let mock = Arc::new(MockWindow::default());
        let injected: Arc<dyn HostWindow> = mock.clone();
        let ctx = WindowContext::new(injected);

        ctx.window.request_redraw();
        ctx.window.set_cursor_visible(false);

        assert_eq!(mock.redraws.load(Ordering::SeqCst), 1, "重绘必须转发");
        assert!(
            !mock.cursor_visible.load(Ordering::SeqCst),
            "光标隐藏必须转发"
        );
        assert_eq!(ctx.window.scale_factor(), 1.5);
        assert!(ctx.window.is_maximized());
    }
}
