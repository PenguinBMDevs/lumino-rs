//! 横 / 纵卷帘共用的输入骨架（T-14b）。
//!
//! 两个方向的事件分发器在几何上不同（键位列在左 vs 底），但「修饰键 / 失焦 /
//! 局部坐标 / 移动消息 / 释放消息」完全一致；先抽出这一层，避免各自再复制。

use lumino_ui_core::Message;
use lumino_ui_core::geom::Point;
use lumino_ui_core::message::{EditorAction, Point2};

use crate::Editor;
use crate::grid::state::GridInteractionState;

/// 局部坐标元组 → `Point`。
pub(super) fn point(local: (f32, f32)) -> Point {
    Point::new(local.0, local.1)
}

/// 同步修饰键状态；返回 `None`（不产生消息）。
pub(super) fn apply_modifiers(
    state: &mut GridInteractionState,
    shift: bool,
    ctrl: bool,
) -> Option<Message> {
    state.shift_pressed = shift;
    state.control_pressed = ctrl;
    None
}

/// 指针移动（画布内才发 `Moved`）。
pub(super) fn moved(editor: &Editor, local: (f32, f32)) -> Option<Message> {
    let local_pos = point(local);
    editor.update_selection_box_animation(Some(local_pos));
    if !editor.is_inside_canvas(local_pos) {
        return None;
    }
    Some(Message::EditorAction(EditorAction::Moved(Point2::new(
        local_pos.x,
        local_pos.y,
    ))))
}

/// 释放：收起选择框动画并发出 `Released`。
pub(super) fn released(editor: &Editor) -> Option<Message> {
    editor.selection_box_anim.set(None);
    Some(Message::EditorAction(EditorAction::Released))
}
