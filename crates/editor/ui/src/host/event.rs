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
/// - `dragging`：画布已有未完成的指针编辑；
/// - `pressed`：本次事件是按下（而不是抬起）。
///
/// 两条语义必须分开：
/// - **按下**永远不落画布——浮层内的按压不得开始新编辑。历史实现把按下也
///   让给「拖拽继续」，一旦画布残留未完成编辑（Release 丢失），之后点任何
///   浮层都会同时落笔，表现为「下拉菜单点一下就穿透到画布」。
/// - **抬起**在拖拽中必须继续——松手可能落在浮层上，否则编辑状态卡死。
pub(crate) fn overlay_blocks_canvas(pointer_blocked: bool, dragging: bool, pressed: bool) -> bool {
    pointer_blocked && (pressed || !dragging)
}

#[cfg(test)]
mod tests {
    use super::overlay_blocks_canvas;

    #[test]
    fn overlay_blocks_canvas_except_during_drag() {
        assert!(
            overlay_blocks_canvas(true, false, true),
            "浮层内不得开始画布交互"
        );
        assert!(
            !overlay_blocks_canvas(true, true, false),
            "已开始的拖拽必须继续，松手可在浮层上"
        );
        assert!(
            overlay_blocks_canvas(true, true, true),
            "按下即使画布有残留编辑也不得穿透浮层"
        );
        assert!(
            !overlay_blocks_canvas(false, false, true),
            "浮层之外照常命中画布"
        );
    }
}
