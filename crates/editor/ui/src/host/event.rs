//! Host 事件处理模块
//!
//! 拆分说明：
//! - `self::keyboard` — 键盘快捷键处理
//! - `self::input` — 鼠标/触摸输入处理
//! - `self::window` — 窗口事件处理、UI 状态管理
//! - `self::message` — 消息分发与处理

pub(crate) mod canvas;
mod input;
mod keyboard;
mod message;
mod window;

use winit;

/// 检查是否按下了 Ctrl 或 Command（macOS）
fn is_ctrl_or_cmd_pressed(modifiers: winit::keyboard::ModifiersState) -> bool {
    modifiers.contains(winit::keyboard::ModifiersState::CONTROL)
        || modifiers.contains(winit::keyboard::ModifiersState::SUPER)
}

/// egui 浮层是否应截断画布输入。
///
/// - `pointer_blocked`：指针落在 egui 占用区（菜单 / 弹层 / band）；
/// - `dragging`：画布已有未完成的指针编辑。
///
/// 已开始的拖拽必须继续——松手可能落在浮层上，否则编辑状态会卡死。
pub(crate) fn overlay_blocks_canvas(pointer_blocked: bool, dragging: bool) -> bool {
    pointer_blocked && !dragging
}

#[cfg(test)]
mod tests {
    use super::overlay_blocks_canvas;

    #[test]
    fn overlay_blocks_canvas_except_during_drag() {
        assert!(overlay_blocks_canvas(true, false), "浮层内不得开始画布交互");
        assert!(
            !overlay_blocks_canvas(true, true),
            "已开始的拖拽必须继续，松手可在浮层上"
        );
        assert!(!overlay_blocks_canvas(false, false), "浮层之外照常命中画布");
    }
}
