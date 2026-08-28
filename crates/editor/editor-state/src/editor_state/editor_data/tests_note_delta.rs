//! 主音轨增量事件（NoteDeltaEvent）专项测试
//!
//! 覆盖：事件记录（拖动/变换）、连续区间合并、dirty 对账（散改/删除/其他轨）。

use std::collections::HashSet;

use lumino_note_core::note::Note;

use super::EditorData;
use crate::editor_state::editor_data::NoteDeltaEvent;

fn make_data(note_count: usize) -> EditorData {
    let notes: Vec<Note> = (0..note_count)
        .map(|i| Note::new((i * 10) as f32, 60 + i as u16, 1.0))
        .collect();
    EditorData::with_f32_notes(1, &notes)
}

/// 提取事件中的 (start_index, len) 列表（断言辅助；仅用于 UpdateRange 测试）
fn event_ranges(events: &[NoteDeltaEvent]) -> Vec<(usize, usize)> {
    events
        .iter()
        .map(|e| match e {
            NoteDeltaEvent::UpdateRange { start_index, notes } => (*start_index, notes.len()),
            _ => panic!("event_ranges 仅支持 UpdateRange"),
        })
        .collect()
}

/// 从事件列表中提取第一个 RemoveAt 的 (index, count)
fn first_remove_at(events: &[NoteDeltaEvent]) -> (usize, usize) {
    for e in events {
        if let NoteDeltaEvent::RemoveAt { index, count } = e {
            return (*index, *count);
        }
    }
    panic!("未找到 RemoveAt 事件");
}

/// 提取事件列表中全部 RemoveAt 的 (index, count) 列表（按记录顺序）
fn all_remove_at(events: &[NoteDeltaEvent]) -> Vec<(usize, usize)> {
    events
        .iter()
        .filter_map(|e| match e {
            NoteDeltaEvent::RemoveAt { index, count } => Some((*index, *count)),
            _ => None,
        })
        .collect()
}

/// 从事件列表中提取所有 InsertAt 的 (index, tick) 摘要
fn insert_at_summary(events: &[NoteDeltaEvent]) -> Vec<(usize, f32)> {
    events
        .iter()
        .filter_map(|e| match e {
            NoteDeltaEvent::InsertAt { index, note } => Some((*index, note.tick)),
            _ => None,
        })
        .collect()
}

// ── record_update_ranges：连续区间合并 ─────────────────────────

#[test]
fn test_record_update_ranges_merges_contiguous() {
    let mut data = make_data(8);
    // 散列索引 [0,1,2,5,6] → 合并为 [0..3) 和 [5..7) 两个事件
    data.record_update_ranges(&[2, 5, 0, 6, 1]);
    let ranges = event_ranges(&data.note_delta_events);
    assert_eq!(ranges, vec![(0, 3), (5, 2)], "连续段合并，升序输出");
    assert!(!data.note_delta_dirty, "记录后清除 dirty");
    assert_eq!(data.track_notes_gen, 1, "整轨同步 bump gen");
    // 事件数据 = notes 对应区间的克隆
    if let NoteDeltaEvent::UpdateRange { notes, .. } = &data.note_delta_events[0] {
        assert_eq!(notes[0].tick, 0.0);
        assert_eq!(notes[2].tick, 20.0);
    }
}

#[test]
fn test_record_update_ranges_empty_noop() {
    let mut data = make_data(3);
    data.record_update_ranges(&[]);
    assert!(data.note_delta_events.is_empty());
    assert!(!data.note_delta_dirty);
    assert_eq!(data.track_notes_gen, 0, "无修改不 bump gen");
}

#[test]
fn test_record_update_ranges_single_contiguous_range() {
    let mut data = make_data(5);
    data.record_update_ranges(&[1, 2, 3]);
    assert_eq!(event_ranges(&data.note_delta_events), vec![(1, 3)]);
}

// ── 拖动：apply_drag_state_streaming 记录事件 ───────────────────

#[test]
fn test_drag_records_update_range_event() {
    use crate::DragState;
    use bit_vec::BitVec;

    let mut data = make_data(4);
    let mut bv = BitVec::from_elem(4, false);
    bv.set(1, true);
    bv.set(2, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(5, -2);

    let modified = data.apply_drag_state_streaming(&ds, 127);
    assert_eq!(modified, 2);
    // 选中 1,2 连续 → 单个 UpdateRange [1, 3)
    assert_eq!(event_ranges(&data.note_delta_events), vec![(1, 2)]);
    assert!(!data.note_delta_dirty, "拖动记录后清除 dirty");
    if let NoteDeltaEvent::UpdateRange { notes, .. } = &data.note_delta_events[0] {
        assert_eq!(notes[0].tick, 15.0, "notes[1] 拖动后 +5");
        assert_eq!(notes[0].key, 59, "notes[1] 原 key 61 → 61-2");
    }
    // document 同步生效（唯一权威源）
    let track = data.track_notes(1);
    assert_eq!(track[1].start_tick as f32, 15.0);
}

#[test]
fn test_drag_scattered_selection_produces_multiple_ranges() {
    use crate::DragState;
    use bit_vec::BitVec;

    let mut data = make_data(8);
    let mut bv = BitVec::from_elem(8, false);
    bv.set(0, true);
    bv.set(3, true);
    bv.set(6, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(5, 0);

    data.apply_drag_state_streaming(&ds, 127);
    assert_eq!(
        event_ranges(&data.note_delta_events),
        vec![(0, 1), (3, 1), (6, 1)]
    );
}

// ── dirty 对账：未知变化 → 全量兜底 ────────────────────────────

#[test]
fn test_unknown_mark_sets_dirty() {
    let mut data = make_data(3);
    data.mark_track_notes_changed();
    assert!(data.note_delta_dirty, "未知来源 mark → 渲染层全量兜底");
}

#[test]
fn test_delete_records_remove_at_event() {
    let mut data = make_data(4);
    data.delete_note_by_index(1);
    // 删除当前音轨 → 记录 RemoveAt 增量事件，不再整轨替换
    assert!(
        !data.note_delta_dirty,
        "已知 current_track 变化走事件增量，不置 dirty"
    );
    assert_eq!(
        first_remove_at(&data.note_delta_events),
        (1, 1),
        "删除索引 1 应产生 RemoveAt {{ index: 1, count: 1 }}"
    );
}

#[test]
fn test_delete_selected_merges_contiguous_into_remove_at_ranges() {
    // 选中 {0, 2, 3, 4, 6}（含一段连续 [2,3,4] 与散点 0/6）：
    // 旧实现对每个选中音符各发一条 RemoveAt{count:1}（5 次段内移位）；
    // 新实现合并连续段为 RemoveAt{index, count}，按降序下发：
    //   [RemoveAt{6,1}, RemoveAt{2,3}, RemoveAt{0,1}]（3 条事件，2 段移位）。
    let mut data = make_data(8);
    let mut selected = HashSet::new();
    selected.insert(0);
    selected.insert(2);
    selected.insert(3);
    selected.insert(4);
    selected.insert(6);

    data.delete_selected_notes(&selected);

    assert!(
        !data.note_delta_dirty,
        "批量删除走事件增量，不置 dirty（渲染层不触发全量兜底重建）"
    );
    let removes = all_remove_at(&data.note_delta_events);
    assert_eq!(
        removes,
        vec![(6, 1), (2, 3), (0, 1)],
        "连续 [2,3,4] 合并为 RemoveAt{{2,3}}，散点各一条；降序下发"
    );
    // 文档剩余音符数 = 8 - 5 = 3，且索引未错位
    assert_eq!(data.current_track_note_count(), 3, "应删除 5 个音符");
    let ticks: Vec<u32> = data
        .current_track_notes()
        .iter()
        .map(|n| n.start_tick)
        .collect();
    assert_eq!(ticks, vec![10, 50, 70], "残留音符应为原索引 1/5/7");
}

#[test]
fn test_delete_selected_empty_is_noop() {
    let mut data = make_data(4);
    data.delete_selected_notes(&HashSet::new());
    assert_eq!(data.current_track_note_count(), 4, "空选中不应删除任何音符");
    assert!(data.note_delta_events.is_empty(), "空选中不应记录增量事件");
}

#[test]
fn test_scattered_edit_records_update_events() {
    // update_note 直接写 document，现在记录 RemoveAt + InsertAt 增量事件
    let mut data = make_data(3);
    data.update_note(data.current_track, 0, Note::new(99.0, 60, 1.0));
    assert!(
        !data.note_delta_dirty,
        "已知 current_track 变化走事件增量，不置 dirty"
    );
    // 事件顺序：RemoveAt(index=0, count=1)，InsertAt(index=2, note)
    let inserts = insert_at_summary(&data.note_delta_events);
    assert_eq!(inserts.len(), 1, "update_note 产生一个 InsertAt 事件");
    assert!(
        matches!(
            data.note_delta_events.first(),
            Some(NoteDeltaEvent::RemoveAt { index: 0, count: 1 })
        ),
        "首事件应为 RemoveAt {{ index: 0, count: 1 }}"
    );
}

#[test]
fn test_other_track_mark_does_not_dirty_main_track() {
    // 洋葱皮音轨（其他轨）编辑 → 主音轨数据未变 → 不置 dirty
    let mut data = make_data(3);
    data.mark_track_notes_changed_for(Some(HashSet::from([2])));
    assert!(!data.note_delta_dirty, "其他轨变化不影响主音轨增量路径");
}

// ── InsertAt 索引：GPU 布局与文档保序（必须 = 文档索引） ─────────

#[test]
fn test_insert_middle_records_doc_index() {
    // 中间插入：文档 [0, 10] 插入 tick=5 → 文档索引 1，InsertAt 索引必须 = 1
    // （旧实现 = partition_point(6)=2，差 1 → GPU 与文档错位，后续增删改全部偏位）
    let mut data = make_data(2);
    assert!(data.insert_note(data.current_track, Note::new(5.0, 60, 1.0)));
    let inserts = insert_at_summary(&data.note_delta_events);
    assert_eq!(
        inserts,
        vec![(1, 5.0)],
        "中间插入：InsertAt 索引 = 文档索引 1"
    );
    // 文档侧验证：新音符确实落在索引 1
    let track = data.track_notes(1);
    assert_eq!(track.len(), 3);
    assert_eq!(track[1].start_tick as f32, 5.0);
}

#[test]
fn test_insert_end_records_doc_index() {
    // 末尾插入：文档 [0, 10] 插入 tick=100 → 文档索引 2（追加），InsertAt 索引 = 2
    let mut data = make_data(2);
    assert!(data.insert_note(data.current_track, Note::new(100.0, 60, 1.0)));
    let inserts = insert_at_summary(&data.note_delta_events);
    assert_eq!(
        inserts,
        vec![(2, 100.0)],
        "末尾插入：InsertAt 索引 = 文档索引 2"
    );
}

#[test]
fn test_insert_same_tick_records_doc_index() {
    // 同 tick 插入：文档 [0, 10] 插入 tick=10 → 稳定插到同 tick 之后 → 文档索引 2
    let mut data = make_data(2);
    assert!(data.insert_note(data.current_track, Note::new(10.0, 61, 1.0)));
    let inserts = insert_at_summary(&data.note_delta_events);
    assert_eq!(
        inserts,
        vec![(2, 10.0)],
        "同 tick 插入：稳定插后，InsertAt 索引 = 2"
    );
    // 事件携带的 note 必须与文档实际音符一致（含 key）
    if let Some(NoteDeltaEvent::InsertAt { note, .. }) = data.note_delta_events.first() {
        assert_eq!(note.key, 61);
    }
}

#[test]
fn test_update_note_move_to_middle_records_remove_and_insert() {
    // 移动音符到中间：update_note 记录 RemoveAt(原索引) + InsertAt(新文档索引)
    let mut data = make_data(3); // [0, 10, 20]
    data.update_note(data.current_track, 0, Note::new(15.0, 60, 1.0));
    // 文档 [10, 15, 20]：新音符索引 1
    let inserts = insert_at_summary(&data.note_delta_events);
    assert_eq!(
        inserts,
        vec![(1, 15.0)],
        "移动到中间：InsertAt 索引 = 新文档索引 1"
    );
    assert!(
        matches!(
            data.note_delta_events.first(),
            Some(NoteDeltaEvent::RemoveAt { index: 0, count: 1 })
        ),
        "首事件应为 RemoveAt {{ index: 0, count: 1 }}"
    );
}

#[test]
fn test_dirty_then_recorded_edit_clears() {
    // 散改置 dirty → 之后完整记录的事件操作清 dirty
    let mut data = make_data(3);
    data.mark_track_notes_changed();
    assert!(data.note_delta_dirty);
    data.record_update_ranges(&[0]);
    assert!(!data.note_delta_dirty, "事件记录后清除 dirty（队列可信任）");
}

// ── 消费：take 清空队列 ────────────────────────────────────────

#[test]
fn test_take_events_clears_queue() {
    let mut data = make_data(3);
    data.record_update_ranges(&[0, 1]);
    let events = data.take_note_delta_events();
    assert_eq!(events.len(), 1);
    assert!(data.note_delta_events.is_empty(), "take 后队列清空");
}

// ── EditorTransform（变速/翻转/移调）记录事件 ───────────────────

#[test]
fn test_flip_vertical_records_event() {
    use crate::EditorTransform;

    let mut data = make_data(4);
    let selected = HashSet::from([1, 2]);
    let modified = data.flip_vertical(&selected, 127.0);
    assert_eq!(modified, 2);
    assert_eq!(event_ranges(&data.note_delta_events), vec![(1, 2)]);
    assert!(!data.note_delta_dirty);
}

#[test]
fn test_apply_speed_change_records_event() {
    use crate::EditorTransform;

    let mut data = make_data(5);
    let selected = HashSet::from([0, 1, 2]);
    let modified = data.apply_speed_change(&selected, 2.0);
    assert_eq!(modified, 3);
    assert_eq!(event_ranges(&data.note_delta_events), vec![(0, 3)]);
    assert!(!data.note_delta_dirty);
}

// ── 边界：越界索引防御 ─────────────────────────────────────────

#[test]
fn test_apply_batch_edit_gate_records_event() {
    let mut data = make_data(4);
    let selected = HashSet::from([1, 2]);
    let modified = data.apply_batch_edit(&selected, "", "*2", "", "", 127);
    assert_eq!(modified, 2);
    assert_eq!(event_ranges(&data.note_delta_events), vec![(1, 2)]);
    assert!(!data.note_delta_dirty);
}

#[test]
fn test_apply_batch_edit_noop_clears_history() {
    let mut data = make_data(3);
    let selected = HashSet::from([0, 1]);
    // 空表达式 → 无变更 → discard_last
    let modified = data.apply_batch_edit(&selected, "", "", "", "", 127);
    assert_eq!(modified, 0);
    assert!(data.note_delta_events.is_empty());
}

#[test]
fn test_record_update_ranges_out_of_bounds_filtered() {
    let mut data = make_data(3);
    // 索引 5 越界 → 事件只含有效区间 [0, 3)
    data.record_update_ranges(&[0, 1, 2, 5, 6]);
    assert_eq!(event_ranges(&data.note_delta_events), vec![(0, 3)]);
}
