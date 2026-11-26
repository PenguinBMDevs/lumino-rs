//! 弯音跳变对（同 tick 多事件）语义测试
//!
//! 覆盖：
//! - PitchBend 同 tick Add 共存（跳变对）、CC 同 tick 替换（唯一性）
//! - 移动跳变对中一个锚点不删除同 tick 另一个（old_value 精确定位）
//! - 移到已存在 tick：PitchBend 共存 / CC 替换

use super::{EditorData, seed_cc, seed_pb};
use lumino_note_core::automation::{AutomationEdit, AutomationTarget, SegmentShape};

#[test]
fn test_pitchbend_add_same_tick_keeps_both() {
    // 弯音跳变对：同 tick 两个事件（直角突变）必须共存——
    // 旧实现"同 tick 替换"会丢掉先创建的锚点（连线绕过中间锚点）
    let mut data = EditorData::new();
    seed_pb(&mut data, 0, 10922);
    seed_pb(&mut data, 1920, 10922);
    // B 正下方创建 C（同 tick 1920）：不得替换 B
    seed_pb(&mut data, 1920, 0);
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
    seed_cc(&mut data);
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
    seed_pb(&mut data, 0, 10922);
    seed_pb(&mut data, 1920, 10922);
    seed_pb(&mut data, 1920, 0);
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
    seed_pb(&mut data, 100, 8192);
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
    seed_cc(&mut data);
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
