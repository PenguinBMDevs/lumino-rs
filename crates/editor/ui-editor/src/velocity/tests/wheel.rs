//! 双向滚轮（水平+垂直同时滚动）单元测试

use crate::velocity::EditMode;
use crate::velocity::widget;
use iced_core::keyboard::Modifiers;
use iced_core::mouse::ScrollDelta;
use iced_widget::canvas::Action;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

/// 辅助：构造指定模式的 VelocityCanvas 与默认状态
fn make_canvas<'a>(editor: &'a crate::Editor, mode: EditMode) -> widget::VelocityCanvas<'a> {
    widget::VelocityCanvas {
        editor,
        edit_mode: mode,
        selected_cc: 1,
    }
}

/// 辅助：从 Action 解包 WheelScrolled 的双轴分量
fn unwrap_wheel_scrolled(action: Action<Message>) -> (f32, f32) {
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::Velocity(VelocityAction::WheelScrolled { delta_x, delta_y })) => {
            (delta_x, delta_y)
        }
        other => panic!("应发布 WheelScrolled，实际为: {other:?}"),
    }
}

/// 对角线触控板滑动（左滑+上滑）：一条消息携带双轴分量
#[test]
fn test_wheel_diagonal_publishes_both_axes() {
    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_wheel_scrolled(
            &state,
            ScrollDelta::Pixels {
                x: -100.0,
                y: -50.0,
            },
        )
        .expect("对角线滚动应产生动作");
    let (delta_x, delta_y) = unwrap_wheel_scrolled(action);
    assert!(
        (delta_x + 100.0).abs() < f32::EPSILON,
        "左滑应产生 delta_x=-100，实际={delta_x}"
    );
    assert!(
        (delta_y + 1.0).abs() < f32::EPSILON,
        "Pixels y=-50 应换算为行单位 -1，实际={delta_y}"
    );
}

/// 纯水平滑动：曾经被直接丢弃，现在发布 WheelScrolled（仅水平分量）
#[test]
fn test_wheel_horizontal_only_publishes_delta_x() {
    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_wheel_scrolled(&state, ScrollDelta::Pixels { x: -60.0, y: 0.0 })
        .expect("纯水平滑动应产生动作");
    let (delta_x, delta_y) = unwrap_wheel_scrolled(action);
    assert!((delta_x + 60.0).abs() < f32::EPSILON);
    assert!((delta_y - 0.0).abs() < f32::EPSILON);
}

/// Lines 源水平分量换算：与钢琴卷帘网格一致（×SCROLL_LINES_SCALE）
#[test]
fn test_wheel_lines_horizontal_uses_scroll_lines_scale() {
    use lumino_ui_core::constants::editor::SCROLL_LINES_SCALE;

    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_wheel_scrolled(&state, ScrollDelta::Lines { x: 1.0, y: 0.0 })
        .expect("Lines 水平滚动应产生动作");
    let (delta_x, delta_y) = unwrap_wheel_scrolled(action);
    assert!(
        (delta_x - SCROLL_LINES_SCALE).abs() < f32::EPSILON,
        "Lines x=1 应换算为 {SCROLL_LINES_SCALE}，实际={delta_x}"
    );
    assert!((delta_y - 0.0).abs() < f32::EPSILON);
}

/// 纯垂直滚动换算保持既有行为（Pixels ÷50 → 行单位）
#[test]
fn test_wheel_vertical_conversion_preserved() {
    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_wheel_scrolled(&state, ScrollDelta::Pixels { x: 0.0, y: 100.0 })
        .expect("垂直滚动应产生动作");
    let (delta_x, delta_y) = unwrap_wheel_scrolled(action);
    assert!((delta_x - 0.0).abs() < f32::EPSILON);
    assert!(
        (delta_y - 2.0).abs() < f32::EPSILON,
        "Pixels y=100 应换算为行单位 2，实际={delta_y}"
    );
}

/// Velocity 模式：非 Ctrl 滚轮仍发布 WheelScrolled（垂直分量由 handler 按模式忽略，水平分量生效）
#[test]
fn test_wheel_velocity_mode_publishes_for_horizontal() {
    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let state = widget::VelocityCanvasState::default();

    let action = canvas
        .handle_wheel_scrolled(&state, ScrollDelta::Pixels { x: -60.0, y: 0.0 })
        .expect("Velocity 模式水平滚动应产生动作");
    let (delta_x, delta_y) = unwrap_wheel_scrolled(action);
    assert!((delta_x + 60.0).abs() < f32::EPSILON);
    assert!((delta_y - 0.0).abs() < f32::EPSILON);
}

/// Ctrl+滚轮（CC 模式）：保持垂直缩放行为，不发布 WheelScrolled
#[test]
fn test_wheel_ctrl_zoom_preserved() {
    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let state = widget::VelocityCanvasState {
        modifiers: Modifiers::CTRL,
        ..Default::default()
    };

    let action = canvas
        .handle_wheel_scrolled(&state, ScrollDelta::Lines { x: 0.0, y: -1.0 })
        .expect("Ctrl+滚轮应产生缩放动作");
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::Velocity(VelocityAction::AutomationZoom(z))) => {
            assert!(
                (z - 0.9).abs() < f32::EPSILON,
                "zoom_delta 应为 1.0+(-1)*0.1=0.9，实际={z}"
            );
        }
        other => panic!("Ctrl+滚轮应发 AutomationZoom，实际为: {other:?}"),
    }
}

/// Ctrl+滚轮（Velocity 模式）：保持无操作
#[test]
fn test_wheel_ctrl_noop_in_velocity_mode() {
    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Velocity);
    let state = widget::VelocityCanvasState {
        modifiers: Modifiers::CTRL,
        ..Default::default()
    };

    let action = canvas.handle_wheel_scrolled(&state, ScrollDelta::Lines { x: 0.0, y: -1.0 });
    assert!(action.is_none(), "Velocity 模式 Ctrl+滚轮应无操作");
}

/// 双轴皆零：无操作
#[test]
fn test_wheel_zero_delta_noop() {
    let editor = crate::Editor::new();
    let canvas = make_canvas(&editor, EditMode::Cc(1));
    let state = widget::VelocityCanvasState::default();

    let action = canvas.handle_wheel_scrolled(&state, ScrollDelta::Pixels { x: 0.0, y: 0.0 });
    assert!(action.is_none(), "零增量应无操作");
}
