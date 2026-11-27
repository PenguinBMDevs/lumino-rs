//! Automation lane Move / CycleShape / Delete / Clear 编辑测试

use super::{EditorData, seed_cc, seed_pb};
use lumino_note_core::automation::{AutomationEdit, SegmentShape};

#[test]
fn test_apply_automation_edit_move() {
    let mut data = EditorData::new();
    seed_cc(&mut data);
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 100,
        new_tick: 200,
        old_value: None,
        new_value: 32,
    });
    assert!(moved);
    assert_eq!(data.automation_lanes[0].events[0].tick, 200);
    assert_eq!(data.automation_lanes[0].events[0].value, 32);
}

#[test]
fn test_apply_automation_edit_cycle_shape() {
    let mut data = EditorData::new();
    seed_cc(&mut data);
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
    seed_cc(&mut data);
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
    seed_cc(&mut data);
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 1,
        lane_idx: 0,
        old_tick: 100,
        new_tick: 200,
        old_value: None,
        new_value: 32,
    });
    assert!(!moved, "should reject move with mismatched track");
}

#[test]
fn test_apply_automation_edit_clear() {
    let mut data = EditorData::new();
    seed_pb(&mut data, 100, 8192);
    let cleared = data.apply_automation_edit(AutomationEdit::Clear {
        track_idx: 0,
        lane_idx: 0,
    });
    assert!(cleared);
    assert!(data.automation_lanes[0].events.is_empty());

    // 空 lane 再次 Clear 返回 false
    let cleared2 = data.apply_automation_edit(AutomationEdit::Clear {
        track_idx: 0,
        lane_idx: 0,
    });
    assert!(!cleared2);

    // lane 保留（不删除 lane 本身）
    assert_eq!(data.automation_lanes.len(), 1);
}

#[test]
fn test_apply_automation_edit_move_same_tick_no_panic() {
    // 回归：old_tick == new_tick（拖拽时 tick 吸附后未变，仅改 value）
    // 旧实现 retain 删除自身导致 pos 越界 panic
    let mut data = EditorData::new();
    seed_pb(&mut data, 100, 8192);
    seed_pb(&mut data, 200, 9000);
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 200,
        new_tick: 200, // 同 tick：仅更新 value
        old_value: None,
        new_value: 9500,
    });
    assert!(moved);
    let lane = &data.automation_lanes[0];
    assert_eq!(lane.events.len(), 2, "同 tick 移动不应丢事件");
    assert_eq!(lane.events[1].tick, 200);
    assert_eq!(lane.events[1].value, 9500);
    // shape 与柄保留（Copy 语义）
    assert_eq!(lane.events[1].shape, SegmentShape::Curve { tension: 0 });
}
