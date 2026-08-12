//! 自动化面板（CC/Bend）工具分发单元测试
//!
//! 覆盖工具行为约束：铅笔/指针工具不操作自动化面板，
//! 自动化面板的编辑交互统一由曲线工具（Curve）负责。

use iced_core::{Point, Size};

use crate::velocity::EditMode;
use lumino_core::Tool;
use lumino_note_core::{AutomationEdit, AutomationTarget, SegmentShape};
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

/// 辅助：构造带 CC 1 自动化锚点数据的 Editor
fn automation_editor_with_cc1() -> crate::Editor {
    let mut editor = crate::Editor::new();
    editor
        .editor_state
        .data
        .apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 1 },
            channel: 0,
            tick: 480,
            value: 64,
            shape: SegmentShape::Step,
        });
    editor
}

/// 自动化锚点 (tick=480, value=64) 在默认视图下的屏幕坐标
///
/// 默认视图：keyboard_width=120, zoom_x=0.1, scroll_x=0；
/// bounds_size=(800, 300) → panel_height=328, h=300。
/// x = 120 + 480*0.1 = 168；y = 28 + 300 - (64/127)*300 ≈ 176.8。
fn automation_anchor_pos() -> Point {
    Point::new(168.0, 176.8)
}

/// 自动化面板点击测试的默认 bounds
fn automation_bounds() -> Size {
    Size::new(800.0, 300.0)
}

/// Pencil 工具点击自动化锚点：不应产生任何操作
#[test]
fn test_automation_pencil_tool_noop() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Pencil);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas.handle_button_pressed(
        &mut state,
        automation_anchor_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        automation_bounds(),
    );
    assert!(action.is_none(), "Pencil 不应操作自动化面板");
    assert!(state.automation_drag.is_none(), "Pencil 不应触发锚点拖拽");
}

/// Pointer 工具点击自动化锚点：同样不操作（自动化交互统一归 Curve）
#[test]
fn test_automation_pointer_tool_noop() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Pointer);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas.handle_button_pressed(
        &mut state,
        automation_anchor_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        automation_bounds(),
    );
    assert!(action.is_none(), "Pointer 不应操作自动化面板");
    assert!(state.automation_drag.is_none());
}

/// Curve 工具点击已有锚点：开始拖拽锚点（MoveAnchor）
#[test]
fn test_automation_curve_tool_drag_anchor() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_button_pressed(
            &mut state,
            automation_anchor_pos(),
            &iced_core::mouse::Cursor::Unavailable,
            automation_bounds(),
        )
        .expect("Curve 命中锚点应产生拖拽动作");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::AutomationDragStart))
    ));
    assert_eq!(
        state.automation_drag,
        Some(widget::AutomationDrag::MoveAnchor { old_tick: 480 })
    );
}

/// Curve 工具点击空白处：开始曲线绘制（CurveDraw）
#[test]
fn test_automation_curve_tool_blank_draw() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();

    // 空白位置：远离锚点 (168, 176.8)
    let action = canvas
        .handle_button_pressed(
            &mut state,
            Point::new(500.0, 100.0),
            &iced_core::mouse::Cursor::Unavailable,
            automation_bounds(),
        )
        .expect("Curve 空白处应开始曲线绘制");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::AutomationDragStart))
    ));
    assert!(matches!(
        state.automation_drag,
        Some(widget::AutomationDrag::CurveDraw { .. })
    ));
}

/// Pencil 双击自动化锚点：不应触发 CycleShape
#[test]
fn test_automation_pencil_double_click_no_cycle_shape() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Pencil);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();
    let bounds = automation_bounds();
    let pos = automation_anchor_pos();

    let first = canvas.handle_button_pressed(
        &mut state,
        pos,
        &iced_core::mouse::Cursor::Unavailable,
        bounds,
    );
    assert!(first.is_none());
    // 第二次点击构成双击：Pencil 不应切换 shape
    let second = canvas.handle_button_pressed(
        &mut state,
        pos,
        &iced_core::mouse::Cursor::Unavailable,
        bounds,
    );
    assert!(second.is_none(), "Pencil 双击不应切换自动化锚点 shape");
}

/// Curve 双击自动化锚点：应触发 CycleShape（编辑交互统一归 Curve）
#[test]
fn test_automation_curve_double_click_cycles_shape() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();
    let bounds = automation_bounds();
    let pos = automation_anchor_pos();

    let _first = canvas.handle_button_pressed(
        &mut state,
        pos,
        &iced_core::mouse::Cursor::Unavailable,
        bounds,
    );
    // 第二次点击构成双击：Curve 应切换 shape
    let second = canvas
        .handle_button_pressed(
            &mut state,
            pos,
            &iced_core::mouse::Cursor::Unavailable,
            bounds,
        )
        .expect("Curve 双击锚点应切换 shape");
    let (message, _, _) = second.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::AutomationEdit(
            AutomationEdit::CycleShape { .. }
        )))
    ));
}

/// 双击检测为 Curve 工具场景：非 Curve 工具的 last_click 不应影响 Curve 判定
#[test]
fn test_automation_double_click_isolated_per_tool() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Pencil);
    let mut state = widget::VelocityCanvasState::default();
    let bounds = automation_bounds();
    let pos = automation_anchor_pos();

    // Pencil 单击一次（不记录到 Curve 双击链）
    let pencil_canvas = make_canvas(&editor, EditMode::Cc(1));
    let _ = pencil_canvas.handle_button_pressed(
        &mut state,
        pos,
        &iced_core::mouse::Cursor::Unavailable,
        bounds,
    );

    // 切换到 Curve 后第一次点击：不应被误判为双击
    editor.set_tool(Tool::Curve);
    let curve_canvas = make_canvas(&editor, EditMode::Cc(1));
    let action = curve_canvas
        .handle_button_pressed(
            &mut state,
            pos,
            &iced_core::mouse::Cursor::Unavailable,
            bounds,
        )
        .expect("Curve 首击应开始拖拽");
    let (message, _, _) = action.into_inner();
    assert!(matches!(
        message,
        Some(Message::Velocity(VelocityAction::AutomationDragStart))
    ));
    assert_eq!(
        state.automation_drag,
        Some(widget::AutomationDrag::MoveAnchor { old_tick: 480 })
    );
}

/// Curve 工具 hover 自动化锚点：应设置悬停高亮
#[test]
fn test_automation_curve_tool_hover_anchor() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Curve);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();

    canvas.handle_cursor_moved(
        &mut state,
        automation_anchor_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        automation_bounds(),
    );
    assert_eq!(state.hover_anchor_tick, Some(480));
}

/// Pencil 工具 hover 自动化锚点：不应设置悬停高亮（不操作自动化面板）
#[test]
fn test_automation_pencil_tool_hover_no_anchor() {
    let mut editor = automation_editor_with_cc1();
    editor.set_tool(Tool::Pencil);
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let mut state = widget::VelocityCanvasState::default();

    canvas.handle_cursor_moved(
        &mut state,
        automation_anchor_pos(),
        &iced_core::mouse::Cursor::Unavailable,
        automation_bounds(),
    );
    assert!(state.hover_anchor_tick.is_none(), "Pencil 悬停不应高亮锚点");
}
