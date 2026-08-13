//! Automation lane 测试 —— find/find_or_create/apply_edit

use lumino_note_core::automation::{AutomationEdit, AutomationTarget, SegmentShape};

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

#[test]
fn test_apply_automation_edit_add_recomputes_auto_handles() {
    // 两个连续事件：自动柄 = 1/3 段长（直线语义）
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 0,
        value: 8192,
        shape: SegmentShape::Curve { tension: 0 },
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 960,
        value: 10000,
        shape: SegmentShape::Curve { tension: 0 },
    });
    let lane = &data.automation_lanes[0];
    assert!(lane.events[0].handles_auto);
    assert_eq!(lane.events[0].out_handle, (320.0, 602.6667));
    assert_eq!(lane.events[1].in_handle, (-320.0, -602.6667));
}

#[test]
fn test_apply_automation_edit_update_handles() {
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 100,
        value: 8192,
        shape: SegmentShape::Curve { tension: 0 },
    });
    let changed = data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 100,
        out_handle: (300.0, 500.0),
        in_handle: (0.0, 0.0),
    });
    assert!(changed);
    let evt = &data.automation_lanes[0].events[0];
    assert_eq!(evt.out_handle, (300.0, 500.0));
    assert!(!evt.handles_auto, "拖柄后标记为自定义");

    // 后续编辑（Add 相邻事件）不覆盖自定义柄
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 960,
        value: 9000,
        shape: SegmentShape::Curve { tension: 0 },
    });
    assert_eq!(
        data.automation_lanes[0].events[0].out_handle,
        (300.0, 500.0)
    );
}

#[test]
fn test_apply_automation_edit_clear() {
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 100,
        value: 8192,
        shape: SegmentShape::Curve { tension: 0 },
    });
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
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 100,
        value: 8192,
        shape: SegmentShape::Curve { tension: 0 },
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 200,
        value: 9000,
        shape: SegmentShape::Curve { tension: 0 },
    });
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 200,
        new_tick: 200, // 同 tick：仅更新 value
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

#[test]
fn test_apply_automation_edit_move_to_existing_tick_replaces() {
    // 移动到已存在的 tick：删除冲突事件后写入
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 100,
        value: 8192,
        shape: SegmentShape::Curve { tension: 0 },
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 200,
        value: 9000,
        shape: SegmentShape::Step,
    });
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 100,
        new_tick: 200, // 与已有事件冲突
        new_value: 7000,
    });
    assert!(moved);
    let lane = &data.automation_lanes[0];
    assert_eq!(lane.events.len(), 1, "冲突事件应被替换");
    assert_eq!(lane.events[0].tick, 200);
    assert_eq!(lane.events[0].value, 7000);
}

#[test]
fn test_apply_automation_edit_update_handles_clamps_loopback() {
    // 越界柄（出向柄越过锚点垂直切线）→ 应用后被钳制，防曲线回环
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 100,
        value: 8192,
        shape: SegmentShape::Curve { tension: 0 },
    });
    let changed = data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 100,
        out_handle: (-300.0, 5000.0), // 越界：柄在锚点左侧
        in_handle: (300.0, -5000.0),  // 越界：柄在锚点右侧
    });
    assert!(changed);
    let evt = &data.automation_lanes[0].events[0];
    assert_eq!(evt.out_handle.0, 0.0, "出向柄被钳制在锚点垂直切线");
    assert_eq!(evt.in_handle.0, 0.0, "入向柄被钳制在锚点垂直切线");
    assert_eq!(evt.out_handle.1, 5000.0, "value 偏移不受限");
    assert!(!evt.handles_auto, "拖柄后标记为自定义");
}

#[test]
fn test_apply_automation_edit_delete_recomputes_handles() {
    // 三事件：删除中间事件后，首尾事件自动柄重算为新的 1/3 段
    let mut data = EditorData::new();
    for (tick, value) in [(0u32, 8192u16), (480, 9000), (960, 10000)] {
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::PitchBend,
            channel: 0,
            tick,
            value,
            shape: SegmentShape::Curve { tension: 0 },
        });
    }
    data.apply_automation_edit(AutomationEdit::Delete {
        track_idx: 0,
        lane_idx: 0,
        tick: 480,
    });
    let lane = &data.automation_lanes[0];
    assert_eq!(lane.events.len(), 2);
    assert_eq!(lane.events[0].out_handle, (320.0, 602.6667));
}
