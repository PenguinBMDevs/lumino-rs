//! Editor 远端光标与 Ctrl 键状态（窗口级可靠通道）
//!
//! 从 `impls/editor_impl.rs` 抽出，控制文件行数并保持单一职责。

use crate::Editor;

impl Editor {
    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    ) {
        self.remote_cursors.insert(
            user_id.to_string(),
            (
                iced_core::Point::new(x, y),
                color.to_string(),
                username.to_string(),
            ),
        );
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: &str) {
        self.remote_cursors.remove(user_id);
        self.grid_cache.clear();
    }

    /// 记录 Ctrl 键按下状态（窗口级 `CtrlKeyChanged` 消息驱动）
    ///
    /// ruler/键盘区 Ctrl+滚轮缩放依赖此字段，走 host 层可靠通道，
    /// 避免 canvas 内 `ModifiersChanged` 事件因焦点问题不送达。
    pub fn set_ctrl_pressed(&mut self, pressed: bool) {
        self.ctrl_pressed = pressed;
    }

    /// 当前 Ctrl 键是否按下（可靠通道）
    pub fn ctrl_pressed(&self) -> bool {
        self.ctrl_pressed
    }

    /// 记录 Shift 键按下状态（窗口级 `ShiftKeyChanged` 消息驱动）
    ///
    /// 形状工具拖拽绘制时据此实时约束为正图形（Shift），走 host 层可靠通道，
    /// 避免 canvas 内 `ModifiersChanged` 事件因焦点问题不送达。
    pub fn set_shift_pressed(&mut self, pressed: bool) {
        self.shift_pressed = pressed;
    }

    /// 当前 Shift 键是否按下（可靠通道）
    pub fn shift_pressed(&self) -> bool {
        self.shift_pressed
    }
}
