//! 删除路径专项测试（自 `tests_note_delta.rs` 拆分，保持各文件 < 400 行）
//!
//! 覆盖：删除增量事件（区间归并 / 阈值回退）、历史语义（快照必须在变更前入栈）、
//! 非当前轨区间队列（`TrackRemoveRanges`）。

use crate::SelectionSet as HashSet;

use lumino_note_core::note::Note;

use super::EditorData;
use crate::editor_state::editor_data::NoteDeltaEvent;

fn make_data(note_count: usize) -> EditorData {
    let notes: Vec<Note> = (0..note_count)
        .map(|i| Note::new((i * 10) as f32, 60 + i as u16, 1.0))
        .collect();
    EditorData::with_f32_notes(1, &notes)
}

/// 从事件列表中提取第一条 `RemoveAt` 的 (index, count)
fn first_remove_at(events: &[NoteDeltaEvent]) -> (usize, usize) {
    for e in events {
        if let NoteDeltaEvent::RemoveAt { index, count } = e {
            return (*index, *count);
        }
    }
    panic!("未找到 RemoveAt 事件");
}

/// 提取事件列表中全部 `RemoveAt` 的 (index, count) 列表（按记录顺序）
fn all_remove_at(events: &[NoteDeltaEvent]) -> Vec<(usize, usize)> {
    events
        .iter()
        .filter_map(|e| match e {
            NoteDeltaEvent::RemoveAt { index, count } => Some((*index, *count)),
            _ => None,
        })
        .collect()
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
    let mut selected = HashSet::default();
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
    data.delete_selected_notes(&HashSet::default());
    assert_eq!(data.current_track_note_count(), 4, "空选中不应删除任何音符");
    assert!(data.note_delta_events.is_empty(), "空选中不应记录增量事件");
}

// ── 历史语义：删除快照必须在变更前入栈（undo 才能恢复）────────────

#[test]
fn test_delete_selected_notes_undo_restores() {
    // 回归：旧实现先删除后 push_history（快照=删除后状态），undo 等于空操作，
    // 删除的音符永久丢失。修复后快照=变更前状态，undo 必须完整恢复。
    let mut data = make_data(8);
    let mut selected = HashSet::default();
    for i in [0, 2, 3, 4, 6] {
        selected.insert(i);
    }
    data.delete_selected_notes(&selected);
    assert_eq!(data.current_track_note_count(), 3);

    assert!(data.undo(), "删除后应存在可撤销条目");
    assert_eq!(
        data.current_track_note_count(),
        8,
        "撤销必须恢复全部被删音符"
    );
    let ticks: Vec<u32> = data
        .current_track_notes()
        .iter()
        .map(|n| n.start_tick)
        .collect();
    assert_eq!(ticks, vec![0, 10, 20, 30, 40, 50, 60, 70], "恢复后顺序无损");
}

#[test]
fn test_delete_note_by_index_undo_restores() {
    let mut data = make_data(4);
    data.delete_note_by_index(1);
    assert_eq!(data.current_track_note_count(), 3);

    assert!(data.undo(), "单音符删除后应存在可撤销条目");
    assert_eq!(data.current_track_note_count(), 4, "撤销必须恢复被删音符");
    let ticks: Vec<u32> = data
        .current_track_notes()
        .iter()
        .map(|n| n.start_tick)
        .collect();
    assert_eq!(ticks, vec![0, 10, 20, 30]);
}

#[test]
fn test_delete_invalid_index_does_not_push_history() {
    let mut data = make_data(4);
    data.delete_note_by_index(99);
    assert_eq!(data.current_track_note_count(), 4, "越界索引不应删除");
    assert!(!data.can_undo(), "无效删除不应产生空撤销步");
    assert!(
        data.note_delta_events.is_empty(),
        "无效删除不应产生渲染侧增量事件"
    );
}

#[test]
fn test_delete_selected_notes_redo_reapplies() {
    let mut data = make_data(8);
    let mut selected = HashSet::default();
    for i in [0, 2, 3, 4, 6] {
        selected.insert(i);
    }
    data.delete_selected_notes(&selected);
    assert!(data.undo());
    assert_eq!(data.current_track_note_count(), 8);

    assert!(data.redo(), "撤销后应可重做删除");
    assert_eq!(data.current_track_note_count(), 3, "重做恢复删除结果");
}

// ── 区间归并 / 阈值回退 / 非当前轨队列 ────────────────────────

#[test]
fn test_remove_notes_merged_current_track_records_remove_at() {
    // 当前轨：区间归并 → `note_delta_events`（主轨段内增量），不置 dirty
    let mut data = make_data(8);
    let deleted = data.remove_notes_merged(data.current_track, &[0, 2, 3, 4, 6]);
    assert_eq!(deleted, 5);
    assert!(!data.note_delta_dirty, "区间删除不得触发全量兜底");
    assert!(
        data.pending_track_remove_ranges.is_empty(),
        "当前轨删除不入非当前轨队列"
    );
    assert_eq!(
        all_remove_at(&data.note_delta_events),
        vec![(6, 1), (2, 3), (0, 1)],
        "连续段合并 + 降序下发"
    );
    assert_eq!(data.current_track_note_count(), 3);
}

#[test]
fn test_remove_notes_merged_other_track_queues_ranges() {
    // 非当前轨：区间归并入 `pending_track_remove_ranges`（UI 转 TrackRemoveRanges），
    // 不写主轨事件、不置 dirty
    let mut data = make_data(4);
    // 扩出第 2 轨（索引 2）并写入 6 个音符
    let other = 2usize;
    data.ensure_track(other);
    let notes: Vec<Note> = (0..6)
        .map(|i| Note::new((i * 10) as f32, 60 + i as u16, 1.0))
        .collect();
    for n in &notes {
        assert!(data.insert_note(other, n.clone()), "插入其他轨音符");
    }
    data.note_delta_events.clear();

    let deleted = data.remove_notes_merged(other, &[5, 3, 4, 0]);
    assert_eq!(deleted, 4);
    assert!(!data.note_delta_dirty);
    assert!(
        data.note_delta_events.is_empty(),
        "非当前轨删除不得写主轨段内事件"
    );
    assert_eq!(
        data.pending_track_remove_ranges,
        vec![(other, vec![(3, 3), (0, 1)])],
        "连续段 [3,4,5] 合并 + 降序"
    );
    assert_eq!(data.track_notes(other).len(), 2, "剩余索引 1/2");
}

#[test]
fn test_remove_ranges_few_keeps_incremental_events() {
    // 区间数在阈值内 → 逐区间 `RemoveAt` 增量（渲染侧段内搬移次数 = 区间数）
    let mut data = make_data(200);
    let indices: Vec<usize> = (0..8).map(|i| i * 20).collect();
    let deleted = data.remove_notes_merged(data.current_track, &indices);
    assert_eq!(deleted, 8);
    assert!(
        !data.main_track_struct_dirty,
        "区间数在阈值内应走逐区间增量，不置段重建标记"
    );
    assert_eq!(data.note_delta_events.len(), 8, "8 个不相邻区间 → 8 条事件");
}

#[test]
fn test_remove_ranges_many_falls_back_to_struct_rebuild() {
    // 区间数超过阈值 → 不产生 O(区间数) 事件（渲染侧每区间一次段尾搬移），
    // 改置主轨段重建标记（单轨 TrackDelta 整段一次，doc 为权威内容）
    let mut data = make_data(200);
    let indices: Vec<usize> = (0..200).step_by(2).collect(); // 100 个不相邻区间
    let deleted = data.remove_notes_merged(data.current_track, &indices);
    assert_eq!(deleted, 100);
    assert!(
        data.main_track_struct_dirty,
        "区间过多应回退主轨段重建（单次整段同步）"
    );
    assert!(
        data.note_delta_events.is_empty(),
        "不得产生 O(区间数) 逐区间事件"
    );
    assert!(
        !data.note_delta_dirty,
        "仍为已知轨增量路径，不触发全量兜底重建"
    );
}

#[test]
fn test_remove_ranges_many_other_track_drops_pending_ranges() {
    // 非当前轨区间过多 → 不排队间（调用方 mark_track_notes_changed_for
    // 已触发整轨 TrackDelta 重建），避免渲染侧 O(区间数 × 段尾) 搬移
    let mut data = make_data(2);
    let other = 2usize;
    data.ensure_track(other);
    let notes: Vec<Note> = (0..200)
        .map(|i| Note::new((i * 10) as f32, 60, 1.0))
        .collect();
    for n in &notes {
        assert!(data.insert_note(other, n.clone()));
    }
    let indices: Vec<usize> = (0..200).step_by(2).collect();
    let deleted = data.remove_notes_merged(other, &indices);
    assert_eq!(deleted, 100);
    assert!(
        data.pending_track_remove_ranges.is_empty(),
        "区间过多时非当前轨不排队间（整轨 Delta 重建替代）"
    );
}

#[test]
fn test_remove_notes_merged_same_track_accumulates_in_order() {
    // 同帧同轨多次调用：后续区间追加（各区间按记录顺序依次应用）
    let mut data = make_data(4);
    let other = 2usize;
    data.ensure_track(other);
    for i in 0..6 {
        assert!(data.insert_note(other, Note::new((i * 10) as f32, 60, 1.0)));
    }
    data.remove_notes_merged(other, &[5]);
    data.remove_notes_merged(other, &[2, 3]);
    assert_eq!(
        data.pending_track_remove_ranges,
        vec![(other, vec![(5, 1), (2, 2)])],
        "同轨多次调用按序追加，不合并错序"
    );
}

#[test]
fn test_take_pending_track_remove_ranges_clears() {
    let mut data = make_data(2);
    let other = 2usize;
    data.ensure_track(other);
    assert!(data.insert_note(other, Note::new(0.0, 60, 1.0)));
    data.remove_notes_merged(other, &[0]);
    let taken = data.take_pending_track_remove_ranges();
    assert_eq!(taken.len(), 1);
    assert!(
        data.pending_track_remove_ranges.is_empty(),
        "take 后队列清空"
    );
}
