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
        }
    }
}
