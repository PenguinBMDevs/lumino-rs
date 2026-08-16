//! 面板宽度拖拽调整处理 — ResizeDragStarted/Dragged/Ended + 公开 resize 方法

use crate::sidebar::core::{MAX_PANEL_WIDTH, MIN_PANEL_WIDTH, Sidebar};

impl Sidebar {
    /// 处理开始拖拽调整面板宽度
    pub(super) fn handle_resize_drag_started(&mut self) {
        self.is_resizing = true;
    }

    /// 处理拖拽中调整面板宽度（无操作，坐标由 start/update 方法处理）
    pub(super) fn handle_resize_dragged(&mut self) {}

    /// 处理结束拖拽调整面板宽度
    pub(super) fn handle_resize_drag_ended(&mut self) {
        self.is_resizing = false;
    }

    /// 检查是否正在调整大小
    pub fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    pub fn start_resize(&mut self, cursor_x: f32) {
        self.is_resizing = true;
        self.resize_start_x = cursor_x;
        self.resize_start_width = self.panel_width;
    }

    pub fn update_resize_position(&mut self, cursor_x: f32) {
        if self.is_resizing {
            let delta_x = cursor_x - self.resize_start_x;
            let new_width = self.resize_start_width + delta_x;
            self.panel_width = new_width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        }
    }

    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }
}
