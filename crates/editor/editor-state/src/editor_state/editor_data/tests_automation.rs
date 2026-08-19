//! Automation lane 测试 —— find/find_or_create/apply_edit
//!
//! 拆分说明（避免单文件超 400 行）：
//! - `tests_automation/move_edit.rs`：Move/CycleShape/Delete/Clear 编辑测试
//! - `tests_automation/jump_pair.rs`：弯音跳变对（同 tick 多事件）语义测试
//! - `tests_automation/handles.rs`：贝塞尔自动柄重算与钳制测试

use lumino_note_core::automation::{AutomationEdit, AutomationTarget, SegmentShape};

use super::EditorData;

mod handles;
mod jump_pair;
mod move_edit;

/// 在 track 0 添加 CC7 事件（tick=100, value=64, Step 形状）——测试常用种子。
fn seed_cc(data: &mut EditorData) {
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
}

/// 在 track 0 添加 PitchBend 事件（Curve 形状）——测试常用种子。
fn seed_pb(data: &mut EditorData, tick: u32, value: u16) {
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick,
        value,
        shape: SegmentShape::Curve { tension: 0 },
    });
}

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
    seed_cc(&mut data);
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
