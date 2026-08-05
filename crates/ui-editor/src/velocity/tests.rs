//! 力度/Tempo/CC 编辑面板单元测试
//!
//! 从 velocity.rs 主文件拆出，因为主文件超过 400 行。

use super::*;
use crate::Note;
use crate::editor_state::ViewState;

// ===== Velocity 测试 =====

/// 将 f32 Note 转为 NoteEvent（测试辅助，与 document 存储格式一致）
fn to_events(notes: &[Note]) -> Vec<lumino_midi_loader::NoteEvent> {
    notes
        .iter()
        .map(|n| {
            lumino_midi_loader::NoteEvent::new(
                n.tick.round() as u32,
                (n.tick + n.length).round() as u32,
                n.key as u8,
                n.velocity,
                n.channel,
            )
        })
        .collect()
}

#[test]
fn test_build_velocity_points_empty() {
    let notes: Vec<lumino_midi_loader::NoteEvent> = Vec::new();
    let points = VelocityPanel::build_velocity_points(&notes);
    assert!(points.is_empty());
}

#[test]
fn test_build_velocity_points_single_note() {
    let notes = to_events(&[Note::new(0.0, 60, 480.0).with_velocity(100)]);
    let points = VelocityPanel::build_velocity_points(&notes);

    assert_eq!(points.len(), 1);
    assert_eq!(points[0].note_index, 0);
    assert_eq!(points[0].tick, 0.0);
    assert_eq!(points[0].velocity, 100);
}

#[test]
fn test_build_velocity_points_multiple_notes() {
    let notes = to_events(&[
        Note::new(480.0, 64, 240.0).with_velocity(80),
        Note::new(0.0, 60, 480.0).with_velocity(100),
        Note::new(960.0, 67, 240.0).with_velocity(120),
        Note::new(480.0, 72, 120.0).with_velocity(60),
    ]);

    let points = VelocityPanel::build_velocity_points(&notes);
    assert_eq!(points.len(), 4);
    assert_eq!(points[0].tick, 0.0);
    assert_eq!(points[0].note_index, 1);
    assert_eq!(points[1].tick, 480.0);
    assert_eq!(points[1].note_index, 0);
    assert_eq!(points[2].tick, 480.0);
    assert_eq!(points[2].note_index, 3);
    assert_eq!(points[3].tick, 960.0);
    assert_eq!(points[3].note_index, 2);
}

// ===== CC 数据测试 =====

#[test]
fn test_build_cc_points_empty() {
    use crate::Editor;
    let editor = Editor::new();
    let points = VelocityPanel::build_cc_points(&editor, 1);
    assert!(points.is_empty());
}

#[test]
fn test_build_cc_points_with_data() {
    use crate::Editor;
    use lumino_note_core::{AutomationEdit, AutomationTarget, SegmentShape};

    let mut editor = Editor::new();
    // 通过 automation_lanes 添加 CC 1 数据（当前音轨为 0）
    for (tick, value) in [(0, 64), (480, 127)] {
        editor
            .editor_state
            .data
            .apply_automation_edit(AutomationEdit::Add {
                track_idx: 0,
                target: AutomationTarget::CC { controller: 1 },
                channel: 0,
                tick,
                value,
                shape: SegmentShape::Step,
            });
    }

    let points = VelocityPanel::build_cc_points(&editor, 1);
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].tick, 0.0);
    assert_eq!(points[0].value, 64);
    assert_eq!(points[1].tick, 480.0);
    assert_eq!(points[1].value, 127);
}

#[test]
fn test_build_cc_points_wrong_number() {
    use crate::Editor;
    use lumino_note_core::{AutomationEdit, AutomationTarget, SegmentShape};

    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 1 },
            channel: 0,
            tick: 0,
            value: 64,
            shape: SegmentShape::Step,
        });

    let points = VelocityPanel::build_cc_points(&editor, 7);
    assert!(points.is_empty(), "不同 CC 号应返回空");
}

// ===== EditMode 测试 =====

#[test]
fn test_edit_mode_default_is_velocity() {
    let mode = EditMode::default();
    assert_eq!(mode, EditMode::Velocity);
}

#[test]
fn test_edit_mode_is_cc() {
    assert!(!EditMode::Velocity.is_cc());
    assert!(EditMode::Cc(1).is_cc());
    assert!(!EditMode::Tempo.is_cc());
}

#[test]
fn test_edit_mode_is_tempo() {
    assert!(!EditMode::Velocity.is_tempo());
    assert!(!EditMode::Cc(1).is_tempo());
    assert!(EditMode::Tempo.is_tempo());
}

#[test]
fn test_edit_mode_display_name() {
    assert_eq!(EditMode::Velocity.display_name(), "力度");
    assert_eq!(EditMode::Tempo.display_name(), "速度");
    assert_eq!(EditMode::Cc(1).display_name(), "CC");
}

// ===== Tempo 数据测试 =====

#[test]
fn test_tempo_bpm_to_y_density_uniform() {
    let height = 200.0;
    let levels = widget::generate_tempo_levels();
    assert_eq!(levels.len(), 9);

    let mut spacings = Vec::new();
    let mut prev_y = widget::tempo_bpm_to_y(levels[0], height);
    for &bpm in levels.iter().skip(1) {
        let point_y = widget::tempo_bpm_to_y(bpm, height);
        spacings.push((prev_y - point_y).abs());
        prev_y = point_y;
    }

    let first = spacings[0];
    for &spacing in &spacings {
        assert!(
            (spacing - first).abs() < f32::EPSILON,
            "等差刻度应在 Y 轴上均匀分布：spacing={spacing}, first={first}"
        );
    }
}

#[test]
fn test_tempo_point_screen_pos_matches_bpm_to_y() {
    let height = 200.0;
    let width = 800.0;
    let view = ViewState::default();
    let point = widget::TempoPoint {
        tick: 0.0,
        bpm: 120.0,
    };
    let pos = widget::tempo_point_screen_pos(&point, width, height, &view, 20.0, 9980.0);
    let expected_y = widget::tempo_bpm_to_y(120.0, height);
    assert!((pos.y - expected_y).abs() < f32::EPSILON);
}

#[test]
fn test_build_tempo_points_no_document() {
    use crate::Editor;
    let editor = Editor::new();
    let points = VelocityPanel::build_tempo_points(&editor);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].tick, 0.0);
    assert!((points[0].bpm - 120.0).abs() < 0.01);
}

#[test]
fn test_build_tempo_points_from_editor_data() {
    use crate::Editor;
    let mut editor = Editor::new();
    // 直接向 tempo_points 写入数据模拟已加载文档
    editor.editor_state.data.tempo_points = vec![
        lumino_note_core::TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        },
        lumino_note_core::TempoPoint {
            tick: 480.0,
            bpm: 140.0,
        },
    ];

    let points = VelocityPanel::build_tempo_points(&editor);
    // 现在 build_tempo_points 从 data.tempo_points 读取，返回编辑后的数据
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].tick, 0.0);
    assert!((points[0].bpm - 120.0).abs() < 0.01);
    assert_eq!(points[1].tick, 480.0);
    assert!((points[1].bpm - 140.0).abs() < 0.01);
}

// ===== 双向滚轮（水平+垂直同时滚动）测试 =====

use iced_core::keyboard::Modifiers;
use iced_core::mouse::ScrollDelta;
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
fn unwrap_wheel_scrolled(action: iced_widget::canvas::Action<Message>) -> (f32, f32) {
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
    let mut state = widget::VelocityCanvasState::default();
    state.modifiers = Modifiers::CTRL;

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
    let mut state = widget::VelocityCanvasState::default();
    state.modifiers = Modifiers::CTRL;

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
