//! 力度/Tempo/CC 编辑面板单元测试
//!
//! 从 velocity.rs 主文件拆出，因为主文件超过 400 行。

use super::*;
use crate::editor::Note;
use crate::editor::editor_state::ViewState;

// ===== Velocity 测试 =====

#[test]
fn test_build_velocity_points_empty() {
    let notes = im::Vector::new();
    let points = VelocityPanel::build_velocity_points(&notes);
    assert!(points.is_empty());
}

#[test]
fn test_build_velocity_points_single_note() {
    let mut notes = im::Vector::new();
    notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
    let points = VelocityPanel::build_velocity_points(&notes);

    assert_eq!(points.len(), 1);
    assert_eq!(points[0].note_index, 0);
    assert_eq!(points[0].tick, 0.0);
    assert_eq!(points[0].velocity, 100);
}

#[test]
fn test_build_velocity_points_multiple_notes() {
    let mut notes = im::Vector::new();
    notes.push_back(Note::new(480.0, 64, 240.0).with_velocity(80));
    notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
    notes.push_back(Note::new(960.0, 67, 240.0).with_velocity(120));
    notes.push_back(Note::new(480.0, 72, 120.0).with_velocity(60));

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
    use crate::editor::Editor;
    let editor = Editor::new();
    let points = VelocityPanel::build_cc_points(&editor, 1);
    assert!(points.is_empty());
}

#[test]
fn test_build_cc_points_with_data() {
    use crate::editor::Editor;
    use lumino_core::{AutomationEdit, AutomationTarget, SegmentShape};

    let mut editor = Editor::new();
    // 通过 automation_lanes 添加 CC 1 数据（当前音轨为 0）
    for (tick, value) in [(0, 64), (480, 127)] {
        editor.editor_state.data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 1 },
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
    use crate::editor::Editor;
    use lumino_core::{AutomationEdit, AutomationTarget, SegmentShape};

    let mut editor = Editor::new();
    editor.editor_state.data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 1 },
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
        let y = widget::tempo_bpm_to_y(bpm, height);
        spacings.push((prev_y - y).abs());
        prev_y = y;
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
    use crate::editor::Editor;
    let editor = Editor::new();
    let points = VelocityPanel::build_tempo_points(&editor);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].tick, 0.0);
    assert!((points[0].bpm - 120.0).abs() < 0.01);
}

#[test]
fn test_build_tempo_points_from_editor_data() {
    use crate::editor::Editor;
    let mut editor = Editor::new();
    // 直接向 tempo_points 写入数据模拟已加载文档
    editor.editor_state.data.tempo_points = vec![
        lumino_core::TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        },
        lumino_core::TempoPoint {
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
