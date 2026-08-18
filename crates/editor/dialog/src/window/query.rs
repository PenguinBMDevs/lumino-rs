//! 对话框窗口 - 查询与状态访问

use std::sync::Arc;

use winit::window::{Window, WindowId};

use super::DialogWindow;

impl DialogWindow {
    /// 获取窗口 ID
    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// 获取底层 winit 窗口引用（用于定位悬浮窗/获取窗口位置）
    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// 设置窗口标题
    pub fn set_window_title(&self, title: &str) {
        self.window.set_title(title);
    }

    /// 请求窗口重绘
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// 是否应关闭
    pub fn should_close(&self) -> bool {
        self.should_close
    }

    /// 请求关闭
    pub fn request_close(&mut self) {
        self.should_close = true;
    }

    /// 获取对话框 UI 的可变引用
    pub fn ui_mut(&mut self) -> Option<&mut lumino_ui::Host> {
        self.ui.as_mut()
    }
}
