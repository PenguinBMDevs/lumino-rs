//! 窗口上下文 —— 从 Host 拆出的窗口/输入相关字段
//!
//! 管理窗口句柄、光标、剪贴板、窗口动作状态。

use iced_core::mouse;
use iced_winit::{Clipboard, winit};
use std::sync::Arc;

use crate::window;

/// 窗口上下文，持有窗口和输入状态。
pub(crate) struct WindowContext {
    /// 窗口句柄
    pub window: Arc<winit::window::Window>,
    /// 光标状态
    pub cursor: mouse::Cursor,
    /// 剪贴板
    pub clipboard: Clipboard,
    /// 逻辑光标位置
    pub cursor_position: Option<iced_core::Point>,
    /// 待处理的窗口动作
    pub pending_window_action: Option<window::TrafficAction>,
    /// 是否正在拖拽
    pub pending_drag: bool,
    /// 工具栏拖拽调整标识
    pub is_toolbar_resizing: bool,
    /// 鼠标按钮按下标识
    pub is_mouse_pressed: bool,
    /// 已实际应用到窗口的光标状态（`None` = 尚未应用；`Some(None)` = 已隐藏）。
    ///
    /// 只在光标图标/可见性**真正变化**时才调用 `set_cursor` / `set_cursor_visible`：
    /// 每帧重复设置会被系统（Windows `WM_SETCURSOR`）与异步设置竞态重置，
    /// 表现为光标在两种形态间闪烁。
    pub applied_cursor: Option<Option<winit::window::CursorIcon>>,
}

impl WindowContext {
    /// 创建窗口上下文
    pub fn new(window: Arc<winit::window::Window>) -> Self {
        let clipboard = Clipboard::connect(Arc::clone(&window));

        Self {
            window,
            cursor: mouse::Cursor::Unavailable,
            clipboard,
            cursor_position: None,
            pending_window_action: None,
            pending_drag: false,
            is_toolbar_resizing: false,
            is_mouse_pressed: false,
            applied_cursor: None,
        }
    }
}
