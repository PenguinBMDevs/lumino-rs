//! CC 数据构建单元测试

use crate::velocity::VelocityPanel;

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
