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
        old_value: None,
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

#[test]
fn test_pitchbend_add_same_tick_keeps_both() {
    // 弯音跳变对：同 tick 两个事件（直角突变）必须共存——
    // 旧实现"同 tick 替换"会丢掉先创建的锚点（连线绕过中间锚点）
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 0,
        value: 10922,
        shape: SegmentShape::Curve { tension: 0 },
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 1920,
        value: 10922,
        shape: SegmentShape::Curve { tension: 0 },
    });
    // B 正下方创建 C（同 tick 1920）：不得替换 B
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 1920,
        value: 0,
        shape: SegmentShape::Curve { tension: 0 },
    });
    let lane = &data.automation_lanes[0];
    assert_eq!(
        lane.events.len(),
        3,
        "同 tick 两个弯音事件必须共存（跳变对）: {:?}",
        lane.events
    );
    // 顺序：稳定排序保持创建顺序——A(0) B(1920,高) C(1920,低)
    let events: Vec<(u32, u16)> = lane.events.iter().map(|e| (e.tick, e.value)).collect();
    assert_eq!(
        events,
        vec![(0, 10922), (1920, 10922), (1920, 0)],
        "弯音事件顺序: {events:?}"
    );
}

#[test]
fn test_cc_add_same_tick_still_replaces() {
    // CC 语义不变：同 tick Add 仍替换（唯一性）
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 127,
        shape: SegmentShape::Step,
    });
    assert_eq!(
        data.automation_lanes[0].events.len(),
        1,
        "CC 同 tick 保持唯一（替换）"
    );
}

#[test]
fn test_pitchbend_move_keeps_same_tick_pair() {
    // 弯音跳变对：移动其中一个锚点（tick 锁定）不得删除同 tick 另一个
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 0,
        value: 10922,
        shape: SegmentShape::Curve { tension: 0 },
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 1920,
        value: 10922,
        shape: SegmentShape::Curve { tension: 0 },
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 0,
        tick: 1920,
        value: 0,
        shape: SegmentShape::Curve { tension: 0 },
    });
    assert_eq!(data.automation_lanes[0].events.len(), 3);
    // 拖动 B（tick 1920 高位锚点）向上 → Move 同 tick 仅更新 value，
    // 用 old_value 精确定位（同 tick 两事件）
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 1920,
        old_value: Some(10922), // 高位锚点
        new_tick: 1920,
        new_value: 12000,
    });
    assert!(moved);
    let lane = &data.automation_lanes[0];
    assert_eq!(lane.events.len(), 3, "Move 不得删除同 tick 跳变对");
    let events: Vec<(u32, u16)> = lane.events.iter().map(|e| (e.tick, e.value)).collect();
    assert_eq!(
        events,
        vec![(0, 10922), (1920, 12000), (1920, 0)],
        "Move 后事件: {events:?}"
    );
    // 拖动低位锚点 C：old_value 精确匹配到 C 而不是 B
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 1920,
        old_value: Some(0), // 低位锚点
        new_tick: 1920,
        new_value: 500,
    });
    assert!(moved);
    let events: Vec<(u32, u16)> = data.automation_lanes[0]
        .events
        .iter()
        .map(|e| (e.tick, e.value))
        .collect();
    assert_eq!(
        events,
        vec![(0, 10922), (1920, 12000), (1920, 500)],
        "old_value 精确定位低位锚点: {events:?}"
    );
}

#[test]
fn test_apply_automation_edit_move_to_existing_tick_pitchbend_keeps_both() {
    // 弯音语义：移到已存在的 tick = 创建跳变对（同 tick 两事件共存，
    // 直角突变）——不替换冲突事件
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
        old_value: None,
        new_value: 7000,
    });
    assert!(moved);
    let lane = &data.automation_lanes[0];
    let events: Vec<(u32, u16)> = lane.events.iter().map(|e| (e.tick, e.value)).collect();
    assert_eq!(
        events,
        vec![(200, 9000), (200, 7000)],
        "弯音移到已有 tick：两事件共存（跳变对）: {events:?}"
    );
}

#[test]
fn test_apply_automation_edit_move_to_existing_tick_cc_replaces() {
    // CC 语义保持：移到已存在的 tick 仍替换（同 tick 唯一）
    let mut data = EditorData::new();
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 200,
        value: 90,
        shape: SegmentShape::Step,
    });
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx: 0,
        old_tick: 100,
        new_tick: 200,
        old_value: None,
        new_value: 70,
    });
    assert!(moved);
    let lane = &data.automation_lanes[0];
    assert_eq!(lane.events.len(), 1, "CC 冲突事件应被替换");
    assert_eq!(lane.events[0].tick, 200);
    assert_eq!(lane.events[0].value, 70);
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
fn test_apply_automation_edit_update_handles_clamps_to_neighbors() {
    // 出向柄拉到下一锚点之外 / 入向柄拉到上一锚点之外 → 钳制到相邻锚点
    // （防贝塞尔 x(t) 非单调导致曲线回环）
    let mut data = EditorData::new();
    for (tick, value) in [(0u32, 8192u16), (960, 8192), (1920, 9000)] {
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::PitchBend,
            channel: 0,
            tick,
            value,
            shape: SegmentShape::Curve { tension: 0 },
        });
    }
    // 事件 0 的出向柄拉到 tick 5000（远超下一事件 960）
    let changed = data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 0,
        out_handle: (5000.0, 4000.0),
        in_handle: (0.0, 0.0),
    });
    assert!(changed);
    assert_eq!(
        data.automation_lanes[0].events[0].out_handle.0, 960.0,
        "出向柄不能越过下一锚点（tick 差 960）"
    );
    assert_eq!(data.automation_lanes[0].events[0].out_handle.1, 4000.0);

    // 事件 2 的入向柄拉到 tick -5000（远超上一事件 960）
    data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 1920,
        out_handle: (0.0, 0.0),
        in_handle: (-5000.0, -3000.0),
    });
    assert_eq!(
        data.automation_lanes[0].events[2].in_handle.0, -960.0,
        "入向柄不能越过上一锚点（tick 差 -960）"
    );
    assert_eq!(data.automation_lanes[0].events[2].in_handle.1, -3000.0);

    // 中间事件 1：两端都钳制
    data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 960,
        out_handle: (5000.0, 0.0),
        in_handle: (-5000.0, 0.0),
    });
    assert_eq!(
        data.automation_lanes[0].events[1].out_handle.0, 960.0,
        "中间锚点出向柄钳制到下一锚点"
    );
    assert_eq!(
        data.automation_lanes[0].events[1].in_handle.0, -960.0,
        "中间锚点入向柄钳制到上一锚点"
    );
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
