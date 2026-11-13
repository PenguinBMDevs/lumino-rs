//! 纵向卷帘的画布输入分派（转置语义：时间轴竖直、音高轴水平、键盘在底部）。
//!
//! 由宿主（winit 事件）直接调用；旧版 `VerticalRollGrid` 的 `update` 只做
//! 事件映射后转调本函数，两条路径共用同一份语义。

use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction};
use lumino_ui_core::Message;
use lumino_ui_core::constants::editor::SCROLL_MAX_DELTA;
use lumino_ui_core::message::{EditorAction, Point2};

use super::{CanvasButton, CanvasEvent};
use crate::Editor;
use crate::grid::program::PianoRollGrid;
use crate::grid::state::GridInteractionState;

/// 分派一条纵向卷帘输入，返回要发布的消息。
pub fn dispatch(
    editor: &Editor,
    state: &mut GridInteractionState,
    event: CanvasEvent,
) -> Option<Message> {
    match event {
        CanvasEvent::Modifiers { shift, ctrl } => {
            state.shift_pressed = shift;
            state.control_pressed = ctrl;
            None
        }
        CanvasEvent::FocusLost => {
            state.shift_pressed = false;
            state.control_pressed = false;
            None
        }
        CanvasEvent::Press { local, button } => match button {
            CanvasButton::Left => left_press(editor, state, local),
            CanvasButton::Right => right_press(editor, local),
        },
        CanvasEvent::Move { local } => move_pointer(editor, local),
        CanvasEvent::Release => {
            editor.selection_box_anim.set(None);
            Some(Message::EditorAction(EditorAction::Released))
        }
        CanvasEvent::Wheel {
            local,
            delta_x,
            delta_y,
            precise,
        } => wheel(editor, state, local, delta_x, delta_y, precise),
    }
}

fn point(local: (f32, f32)) -> lumino_ui_core::geom::Point {
    lumino_ui_core::geom::Point::new(local.0, local.1)
}

/// 纵向卷帘的光标形态（复用 `vertical_mouse` 的判定）。
pub fn cursor(
    editor: &Editor,
    state: &GridInteractionState,
) -> lumino_ui_core::geom::mouse::Interaction {
    crate::grid::vertical_mouse::handle(editor, state.position)
}

/// 左键：悬浮层列表优先，其次画布内按下（双击 / 单击）。
fn left_press(
    editor: &Editor,
    state: &mut GridInteractionState,
    local: (f32, f32),
) -> Option<Message> {
    let local_pos = point(local);
    if let Some(action) = crate::grid::shape_list_box::press_action(editor, local_pos) {
        return action.map(Message::EditorAction);
    }
    if let Some(action) = crate::grid::area_paint_box::press_action(editor, local_pos) {
        return action.map(Message::EditorAction);
    }
    if !editor.is_inside_canvas(local_pos) {
        return None;
    }
    let grid = PianoRollGrid::new(editor);
    if grid.detect_double_click(state, local_pos) {
        return Some(Message::EditorAction(EditorAction::DoubleClicked(
            Point2::new(local_pos.x, local_pos.y),
        )));
    }
    Some(Message::EditorAction(EditorAction::Pressed {
        pos: Point2::new(local_pos.x, local_pos.y),
        shift: state.shift_pressed,
        ctrl: state.control_pressed,
    }))
}

/// 右键：关闭图形列表 / 清除区域着色 / 打开上下文菜单。
fn right_press(editor: &Editor, local: (f32, f32)) -> Option<Message> {
    let local_pos = point(local);
    if editor.shape_list_open {
        return Some(Message::EditorAction(EditorAction::ShapeListClose));
    }
    if let Some(action) = crate::grid::area_paint_box::clear_action(editor, local_pos) {
        return Some(Message::EditorAction(action));
    }
    if !editor.is_inside_canvas(local_pos) {
        return None;
    }
    let target = match editor.drawn_shape_pending_at_screen(local_pos) {
        Some(true) => editor
            .pending_ref_at_screen(local_pos)
            .filter(|pending| editor.pending_menu_allowed(*pending))
            .map_or(ContextMenuTarget::Notes, |_| {
                ContextMenuTarget::PendingDrawnShape
            }),
        Some(false) if editor.current_tool() == lumino_message::Tool::ShapeSelect => {
            ContextMenuTarget::DrawnShape
        }
        _ => ContextMenuTarget::Notes,
    };
    Some(Message::PianoRollContextMenu(
        PianoRollContextMenuAction::Open {
            position: Point2::new(local_pos.x, local_pos.y),
            target,
        },
    ))
}

/// 指针移动：框选平滑动画 + `Moved`（画布内）。
fn move_pointer(editor: &Editor, local: (f32, f32)) -> Option<Message> {
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

/// 滚轮：键盘区缩放音高、网格区缩放时间（Ctrl）或滚动（双向映射）。
fn wheel(
    editor: &Editor,
    state: &GridInteractionState,
    local: (f32, f32),
    delta_x: f32,
    delta_y: f32,
    precise: bool,
) -> Option<Message> {
    let es = &editor.editor_state;
    let canvas_w = es.canvas.size_x.max(1.0);
    let canvas_h = es.canvas.size_y.max(1.0);
    let view = &es.view;
    // 纵向：键盘在底部（高度 = keyboard_width），无顶部标尺。
    let grid_bottom = (canvas_h - view.keyboard_width).max(1.0);
    let grid_h = grid_bottom;

    // 键盘区（底部）：便捷缩放音高 Y。
    if local.1 >= grid_bottom {
        let factor = crate::zoom::zoom_factor_from_wheel(delta_y, precise)?;
        return Some(Message::ZoomYChanged {
            zoom: view.zoom_y * factor,
            fixed_ratio: (local.0 / canvas_w).clamp(0.0, 1.0),
        });
    }

    // 网格区：Ctrl 缩放时间轴，否则双向滚动。
    let ctrl_pressed = state.control_pressed || editor.ctrl_pressed();
    if ctrl_pressed {
        let factor = crate::zoom::zoom_factor_from_wheel(delta_y, precise)?;
        // 锚点距底部比例：0 = 键盘顶部，1 = 顶部。
        let fixed_ratio = ((grid_bottom - local.1) / grid_h).clamp(0.0, 1.0);
        return Some(Message::ZoomXChanged {
            zoom: view.zoom_x * factor,
            fixed_ratio,
        });
    }

    let (dx, dy) = crate::grid::program::wheel_delta(delta_x, delta_y, precise);
    let limit = if precise { 400.0 } else { SCROLL_MAX_DELTA };
    let mut out_x = 0.0;
    let mut out_y = 0.0;
    // 垂直增量 → 时间轴（取反：向上滚显示更后时间）。
    if dy.abs() > f32::EPSILON {
        out_x = (-dy).clamp(-limit, limit);
    }
    // 水平增量 → 音高轴（取反：向右滚显示更高音）。
    if dx.abs() > f32::EPSILON {
        out_y = (-dx).clamp(-limit, limit);
    }
    // Shift+滚轮：垂直转水平（触控板兼容）。
    if state.shift_pressed && out_x.abs() < f32::EPSILON && dy.abs() > f32::EPSILON {
        out_y = (-dy).clamp(-limit, limit);
        out_x = 0.0;
    }
    if out_x.abs() < f32::EPSILON && out_y.abs() < f32::EPSILON {
        return None;
    }
    Some(Message::EditorAction(EditorAction::Scrolled {
        delta_x: out_x,
        delta_y: out_y,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;
    use crate::tests::test_helpers::seed_notes;
    use lumino_ui_core::Message;
    use lumino_ui_core::message::EditorAction;

    /// 纵向卷帘 + 固定画布几何（局部坐标 = 画布坐标）。
    fn editor() -> Editor {
        let mut editor = Editor::new();
        seed_notes(&mut editor, 2, 1, &[]);
        editor.editor_state.is_vertical_roll = true;
        editor.set_canvas_size(lumino_ui_core::geom::Point::new(2000.0, 3000.0));
        editor.set_canvas_offset(lumino_ui_core::geom::Point::new(0.0, 0.0));
        editor
    }

    /// 网格区滚轮：垂直增量转置为时间轴滚动（向下滚 → delta_x > 0）。
    #[test]
    fn grid_wheel_scrolls_time_axis() {
        let editor = editor();
        let mut state = GridInteractionState::default();
        let message = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Wheel {
                local: (500.0, 500.0),
                delta_x: 0.0,
                delta_y: -1.0,
                precise: false,
            },
        );
        match message {
            Some(Message::EditorAction(EditorAction::Scrolled { delta_x, delta_y })) => {
                assert!(delta_x > 0.0, "向下滚应推进时间轴，实际 delta_x={delta_x}");
                assert_eq!(delta_y, 0.0, "纯垂直滚动不应移动音高轴");
            }
            other => panic!("网格区滚轮应发 Scrolled，实际：{other:?}"),
        }
    }

    /// 键盘区（底部）滚轮：缩放音高 Y。
    #[test]
    fn keyboard_wheel_zooms_pitch() {
        let editor = editor();
        let mut state = GridInteractionState::default();
        let keyboard_h = editor.editor_state.view.keyboard_width;
        let message = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Wheel {
                local: (500.0, 3000.0 - keyboard_h * 0.5),
                delta_x: 0.0,
                delta_y: 1.0,
                precise: false,
            },
        );
        assert!(
            matches!(message, Some(Message::ZoomYChanged { .. })),
            "键盘区滚轮应缩放音高，实际：{message:?}"
        );
    }

    /// 左键按下：画布内产生 `Pressed`，并带上 Ctrl 状态。
    #[test]
    fn left_press_inside_canvas_emits_pressed() {
        let editor = editor();
        let mut state = GridInteractionState::default();
        dispatch(
            &editor,
            &mut state,
            CanvasEvent::Modifiers {
                shift: false,
                ctrl: true,
            },
        );
        let message = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Press {
                local: (600.0, 400.0),
                button: CanvasButton::Left,
            },
        );
        match message {
            Some(Message::EditorAction(EditorAction::Pressed { pos, ctrl, .. })) => {
                assert_eq!((pos.x, pos.y), (600.0, 400.0));
                assert!(ctrl, "Ctrl 必须随按下一起传给编辑动作");
            }
            other => panic!("画布内按下应发 Pressed，实际：{other:?}"),
        }
    }
}
