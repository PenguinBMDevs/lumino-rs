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

/// REND-003 回归：批量导入控制事件（`import_control_events_from_document`）语义。
///
/// 覆盖：CC 同 tick 覆盖（后出现者胜）、PitchBend 同 tick 多条（跳变对）、
/// `lane.channel` 取该 lane 最后一条事件、事件按 tick 升序、lane 数 = 出现的组合数。
#[test]
fn test_batch_import_control_events_semantics() {
    use lumino_midi_model::{ChunkedList, MidiDocument, PackedControlEvent, TrackManager};

    let control_events = ChunkedList::from_sorted(vec![
        PackedControlEvent::control_change(5, 1, 3, 11, 64),
        PackedControlEvent::control_change(10, 0, 0, 7, 100),
        PackedControlEvent::control_change(10, 0, 0, 7, 120), // 同 tick → 覆盖为 120
        PackedControlEvent::control_change(20, 0, 0, 7, 80),
        PackedControlEvent::pitch_bend(30, 0, 0, 8192),
        PackedControlEvent::pitch_bend(30, 0, 0, 4096), // 同 tick 保留两条
    ]);
    let doc = MidiDocument {
        notes: vec![ChunkedList::new(), ChunkedList::new()],
        next_note_id: 1,
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events,
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T0".to_string()), Some("T1".to_string())],
        total_ticks: 100,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![0, 0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(2),
    };

    let mut data = EditorData::new();
    data.import_control_events_from_document(&doc);

    assert_eq!(
        data.automation_lanes.len(),
        3,
        "应有 3 条 lane：(1,CC11) / (0,CC7) / (0,PB)"
    );

    let cc7 = data
        .find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
        .expect("CC7 lane 应存在");
    let lane = &data.automation_lanes[cc7];
    assert_eq!(lane.channel, 0);
    assert_eq!(
        lane.events
            .iter()
            .map(|e| (e.tick, e.value))
            .collect::<Vec<_>>(),
        vec![(10, 120), (20, 80)],
        "CC 同 tick 应后出现者胜，且按 tick 升序"
    );

    let pb = data
        .find_automation_lane(0, &AutomationTarget::PitchBend)
        .expect("PB lane 应存在");
    assert_eq!(
        data.automation_lanes[pb]
            .events
            .iter()
            .map(|e| (e.tick, e.value))
            .collect::<Vec<_>>(),
        vec![(30, 8192), (30, 4096)],
        "PitchBend 同 tick 应保留多条（跳变对）"
    );

    let cc11 = data
        .find_automation_lane(1, &AutomationTarget::CC { controller: 11 })
        .expect("CC11 lane 应存在");
    let lane = &data.automation_lanes[cc11];
    assert_eq!(lane.channel, 3, "通道取该 lane 最后一条事件的通道");
    assert_eq!(
        lane.events
            .iter()
            .map(|e| (e.tick, e.value))
            .collect::<Vec<_>>(),
        vec![(5, 64)]
    );
}

/// REND-003 规模回归：单 lane 20 万条 CC 的批量导入必须保持线性。
///
/// 旧逐条路径在此规模下是 O(M² log M)（≈4×10^10 量级操作，分钟级），
/// 若未来有人改回逐条 `apply_automation_edit`，本测试会在 CI 超时暴露。
#[test]
fn test_batch_import_control_events_scales_linearly() {
    use lumino_midi_model::{ChunkedList, MidiDocument, PackedControlEvent, TrackManager};

    const N: u32 = 200_000;
    let events: Vec<PackedControlEvent> = (0..N)
        .map(|i| PackedControlEvent::control_change(i * 2, 0, 0, 7, (i % 128) as u8))
        .collect();
    let doc = MidiDocument {
        notes: vec![ChunkedList::new()],
        next_note_id: 1,
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::from_sorted(events),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T0".to_string())],
        total_ticks: N * 2,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
    };

    let mut data = EditorData::new();
    data.import_control_events_from_document(&doc);

    assert_eq!(data.automation_lanes.len(), 1);
    assert_eq!(data.automation_lanes[0].events.len(), N as usize);
    assert!(
        data.automation_lanes[0]
            .events
            .windows(2)
            .all(|w| w[0].tick < w[1].tick),
        "事件必须按 tick 严格升序"
    );
}
