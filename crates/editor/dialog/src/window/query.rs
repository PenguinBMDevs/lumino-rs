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

    /// 取出该对话框待全局应用的主题（仅设置对话框会产生）
    ///
    /// 设置面板切换主题后需立即同步主窗口与所有对话框（而非等确认按钮），
    /// 由 Runner 逐帧消费本方法的返回值。
    pub fn take_pending_theme_apply(&mut self) -> Option<String> {
        self.ui
            .as_mut()
            .and_then(|ui| ui.take_pending_theme_apply())
    }
}
