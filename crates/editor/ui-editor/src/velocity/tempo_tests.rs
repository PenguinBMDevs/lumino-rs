//! Tempo 面板工具分发单元测试
//!
//! 覆盖工具行为约束：铅笔/指针工具不操作 Tempo 面板，
//! Tempo 面板的编辑交互统一由曲线工具（Curve）负责。

use iced_core::{Point, Size};

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

/// 默认 Tempo 数据：单个速度点 (tick=0, bpm=120)
///
/// 屏幕坐标计算（bounds_size=(800,300)，view 默认 zoom_x=0.1/keyboard_width=120）：
/// x = 0*0.1 + 120 = 120；
/// y = tempo_bpm_to_y(120, 512, 300) ≈ 300 - (120-20)/(512-20)*(300-17) ≈ 242.5。
fn tempo_point_pos() -> Point {
    Point::new(120.0, 242.5)
}

/// Tempo 面板点击测试的默认 bounds
fn tempo_bounds() -> Size {
    Size::new(800.0, 300.0)
}

/// Pencil 工具点击 Tempo 速度点：不应产生任何操作
#[test]
fn test_tempo_pencil_tool_noop() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Pencil);
    let canvas = make_canvas(&editor, EditMode::Tempo);
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas.handle_button_pressed(
        &mut state,
        tempo_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        tempo_bounds(),
    );
    assert!(action.is_none(), "Pencil 不应操作 Tempo 面板");
    assert!(state.tempo_drag_idx.is_none(), "Pencil 不应触发速度点拖拽");
}

/// Pointer 工具点击 Tempo 速度点：同样不操作（Tempo 交互统一归 Curve）
#[test]
fn test_tempo_pointer_tool_noop() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Pointer);
    let canvas = make_canvas(&editor, EditMode::Tempo);
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas.handle_button_pressed(
        &mut state,
        tempo_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        tempo_bounds(),
    );
    assert!(action.is_none(), "Pointer 不应操作 Tempo 面板");
    assert!(state.tempo_drag_idx.is_none());
}

/// Curve 工具点击已有速度点：开始拖拽（TempoDragStart）
#[test]
fn test_tempo_curve_tool_drag_point() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Tempo);
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_button_pressed(
            &mut state,
            tempo_point_pos(),
            &iced_core::mouse::Cursor::Unavailable,
            tempo_bounds(),
        )
        .expect("Curve 命中速度点应产生拖拽动作");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::TempoDragStart(0)))
    ));
    assert_eq!(state.tempo_drag_idx, Some(0));
}

/// Curve 工具点击空白处：创建新速度点（TempoAdd）
#[test]
fn test_tempo_curve_tool_blank_creates_point() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Tempo);
    let mut state = widget::VelocityCanvasState::default();

    // 空白位置：远离速度点 (120, 242.5)
    let action = canvas
        .handle_button_pressed(
            &mut state,
            Point::new(400.0, 100.0),
            &iced_core::mouse::Cursor::Unavailable,
            tempo_bounds(),
        )
        .expect("Curve 空白处应创建新速度点");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::TempoAdd(_, _)))
    ));
}

/// Eraser 工具点击速度点：仍可删除（删除是擦除工具的通用职责）
#[test]
fn test_tempo_eraser_tool_delete() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Eraser);
    let canvas = make_canvas(&editor, EditMode::Tempo);
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_button_pressed(
            &mut state,
            tempo_point_pos(),
            &iced_core::mouse::Cursor::Unavailable,
            tempo_bounds(),
        )
        .expect("Eraser 命中速度点应删除");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::TempoDelete(0)))
    ));
}

/// Curve 工具 hover 速度点：应设置悬停高亮
#[test]
fn test_tempo_curve_tool_hover_point() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Tempo);
    let mut state = widget::VelocityCanvasState::default();

    canvas.handle_cursor_moved(
        &mut state,
        tempo_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        tempo_bounds(),
    );
    assert_eq!(state.tempo_hover_idx, Some(0));
}

/// Pencil 工具 hover 速度点：不应设置悬停高亮（不操作 Tempo 面板）
#[test]
fn test_tempo_pencil_tool_hover_no_point() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Pencil);
    let canvas = make_canvas(&editor, EditMode::Tempo);
    let mut state = widget::VelocityCanvasState::default();

    canvas.handle_cursor_moved(
        &mut state,
        tempo_point_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        tempo_bounds(),
    );
    assert!(state.tempo_hover_idx.is_none(), "Pencil 悬停不应高亮速度点");
}
