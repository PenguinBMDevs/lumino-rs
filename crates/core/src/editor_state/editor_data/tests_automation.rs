//! Automation lane 测试 —— find/find_or_create/apply_edit

use crate::automation::{AutomationEdit, AutomationTarget, SegmentShape};

use super::EditorData;

#[test]
fn test_find_automation_lane_returns_none() {
    let data = EditorData::new();
    assert!(
        data.find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
            .is_none()
    );
}

#[test]
fn test_find_or_create_automation_lane_creates_new() {
    let mut data = EditorData::new();
    let idx = data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
    assert_eq!(idx, 0, "first lane gets index 0");
    assert_eq!(data.automation_lanes.len(), 1);
    assert_eq!(
        data.automation_lanes[0].target,
        AutomationTarget::CC { controller: 7 }
    );
    assert_eq!(data.automation_lanes[0].track, 0);
}

#[test]
fn test_find_or_create_automation_lane_reuses_existing() {
    let mut data = EditorData::new();
    let idx1 = data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
    let idx2 = data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
    assert_eq!(idx1, idx2, "same lane should be reused");
    assert_eq!(data.automation_lanes.len(), 1);
}

#[test]
fn test_apply_automation_edit_add() {
    let mut data = EditorData::new();
    let added = data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    assert!(added);
    assert_eq!(data.automation_lanes.len(), 1);
    assert_eq!(data.automation_lanes[0].events.len(), 1);
    assert_eq!(data.automation_lanes[0].events[0].tick, 100);
    assert_eq!(data.automation_lanes[0].events[0].value, 64);
}

#[test]
fn test_apply_automation_edit_add_duplicate_tick_replaces() {
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    let replaced = data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 127,
        shape: SegmentShape::Curve { tension: 0 },
    });
    assert!(replaced);
    assert_eq!(
        data.automation_lanes[0].events.len(),
        1,
        "same tick replaces"
    );
    assert_eq!(data.automation_lanes[0].events[0].value, 127);
}

#[test]
fn test_apply_automation_edit_move() {
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 100,
        new_tick: 200,
        new_value: 32,
    });
    assert!(moved);
    assert_eq!(data.automation_lanes[0].events[0].tick, 200);
    assert_eq!(data.automation_lanes[0].events[0].value, 32);
}

#[test]
fn test_apply_automation_edit_cycle_shape() {
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    let cycled = data.apply_automation_edit(AutomationEdit::CycleShape {
        track_idx: 0,
        lane_idx: 0,
        tick: 100,
    });
    assert!(cycled);
    assert_eq!(
        data.automation_lanes[0].events[0].shape,
        SegmentShape::Curve { tension: 0 }
    );

    let cycled2 = data.apply_automation_edit(AutomationEdit::CycleShape {
        track_idx: 0,
        lane_idx: 0,
        tick: 100,
    });
    assert!(cycled2);
    assert_eq!(data.automation_lanes[0].events[0].shape, SegmentShape::Step);
}

#[test]
fn test_apply_automation_edit_delete() {
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    let deleted = data.apply_automation_edit(AutomationEdit::Delete {
        track_idx: 0,
        lane_idx: 0,
        tick: 100,
    });
    assert!(deleted);
    assert!(data.automation_lanes[0].events.is_empty());

    let deleted2 = data.apply_automation_edit(AutomationEdit::Delete {
        track_idx: 0,
        lane_idx: 0,
        tick: 999,
    });
    assert!(!deleted2);
}

#[test]
fn test_apply_automation_edit_move_wrong_track_returns_false() {
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 1,
        lane_idx: 0,
        old_tick: 100,
        new_tick: 200,
        new_value: 32,
    });
    assert!(!moved, "should reject move with mismatched track");
}
