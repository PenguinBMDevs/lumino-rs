//! 力度面板（Velocity 模式）工具分发单元测试
//!
//! 覆盖工具行为约束：铅笔/指针工具不操作力度面板，
//! 力度面板的编辑交互统一由曲线工具（Curve）负责。

use iced_core::{Point, Size};

use crate::Note;
use crate::velocity::EditMode;
use lumino_core::Tool;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::widget;

/// 辅助：构造指定模式的 VelocityCanvas 与默认状态
fn make_canvas<'a>(editor: &'a crate::Editor, mode: EditMode) -> widget::VelocityCanvas<'a> {
    widget::VelocityCanvas {
        editor,
        edit_mode: mode,
        selected_cc: 1,
    }
}

/// 辅助：构造含单个音符（tick=0, key=60, length=480, velocity=100）的 Editor
fn velocity_editor_with_note() -> crate::Editor {
    let mut editor = crate::Editor::new();
    crate::tests::test_helpers::seed_single_track(
        &mut editor,
        &[Note::new(0.0, 60, 480.0).with_velocity(100)],
    );
    editor
}

/// 力度点 (tick=0, velocity=100) 在默认视图下的屏幕坐标
///
/// 默认视图：keyboard_width=120, zoom_x=0.1, scroll_x=0；
/// bounds_size=(800, 300)，min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT = 17。
/// x = 0*0.1 + 120 = 120；y = 300 - (100/127)*(300-17) ≈ 77.2。
fn velocity_point_pos() -> Point {
    Point::new(120.0, 77.2)
}

/// 力度面板点击测试的默认 bounds
fn velocity_bounds() -> Size {
    Size::new(800.0, 300.0)
}

/// Pencil 工具点击力度点：不应产生任何操作
#[test]
fn test_velocity_pencil_tool_noop() {
    let mut editor = velocity_editor_with_note();
    editor.set_tool(Tool::Pencil);
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas.handle_button_pressed(
        &mut state,
        velocity_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        velocity_bounds(),
    );
    assert!(action.is_none(), "Pencil 不应操作力度面板");
    assert!(state.drag_point_idx.is_none(), "Pencil 不应触发力度点拖拽");
    assert!(!state.curve_active, "Pencil 不应进入力度曲线绘制");
}

/// Pointer 工具点击力度点：同样不操作（力度交互统一归 Curve）
#[test]
fn test_velocity_pointer_tool_noop() {
    let mut editor = velocity_editor_with_note();
    editor.set_tool(Tool::Pointer);
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas.handle_button_pressed(
        &mut state,
        velocity_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        velocity_bounds(),
    );
    assert!(action.is_none(), "Pointer 不应操作力度面板");
    assert!(state.drag_point_idx.is_none());
}

/// Curve 工具点击已有力度点：开始拖拽（DragStart）
#[test]
fn test_velocity_curve_tool_drag_point() {
    let mut editor = velocity_editor_with_note();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_button_pressed(
            &mut state,
            velocity_point_pos(),
            &iced_core::mouse::Cursor::Unavailable,
            velocity_bounds(),
        )
        .expect("Curve 命中力度点应产生拖拽动作");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::DragStart(0, 100)))
    ));
    assert_eq!(state.drag_point_idx, Some(0));
}

/// Curve 工具点击空白处：进入力度曲线绘制（CurveStart）
#[test]
fn test_velocity_curve_tool_blank_draw() {
    let mut editor = velocity_editor_with_note();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let mut state = widget::VelocityCanvasState::default();

    // 空白位置：远离力度点 (120, 77.2)
    let action = canvas
        .handle_button_pressed(
            &mut state,
            Point::new(500.0, 200.0),
            &iced_core::mouse::Cursor::Unavailable,
            velocity_bounds(),
        )
        .expect("Curve 空白处应进入力度曲线绘制");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::CurveStart))
    ));
    assert!(state.curve_active);
}

/// Curve 工具 hover 力度点：应设置悬停高亮
#[test]
fn test_velocity_curve_tool_hover_point() {
    let mut editor = velocity_editor_with_note();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let mut state = widget::VelocityCanvasState::default();

    canvas.handle_cursor_moved(
        &mut state,
        velocity_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        velocity_bounds(),
    );
    assert_eq!(state.hover_point_idx, Some(0));
}

/// Pencil 工具 hover 力度点：不应设置悬停高亮（不操作力度面板）
#[test]
fn test_velocity_pencil_tool_hover_no_point() {
    let mut editor = velocity_editor_with_note();
    editor.set_tool(Tool::Pencil);
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let mut state = widget::VelocityCanvasState::default();

    canvas.handle_cursor_moved(
        &mut state,
        velocity_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        velocity_bounds(),
    );
    assert!(state.hover_point_idx.is_none(), "Pencil 悬停不应高亮力度点");
}
