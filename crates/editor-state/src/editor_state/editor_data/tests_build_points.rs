//! build_cc_points / build_bend_points 测试

use lumino_note_core::automation::{AutomationEdit, AutomationTarget, SegmentShape};
use lumino_note_core::midi_types::PITCH_BEND_CENTER;

use super::EditorData;

#[test]
fn test_build_cc_points_empty() {
    let data = EditorData::new();
    let points = data.build_cc_points(7);
    assert!(points.is_empty());
}

#[test]
fn test_build_cc_points_with_data() {
    let mut data = EditorData::new();
    data.current_track = 0;
    data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    let points = data.build_cc_points(7);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].tick, 100.0);
    assert_eq!(points[0].value, 64);
}

#[test]
fn test_build_bend_points_with_data() {
    let mut data = EditorData::new();
    data.current_track = 0;
    data.find_or_create_automation_lane(0, AutomationTarget::PitchBend);
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 100,
        value: PITCH_BEND_CENTER as u16,
        shape: SegmentShape::Curve { tension: 0 },
    });
    let points = data.build_bend_points();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].tick, 100.0);
    assert_eq!(points[0].value, 0, "PITCH_BEND_CENTER → center = 0");
}
