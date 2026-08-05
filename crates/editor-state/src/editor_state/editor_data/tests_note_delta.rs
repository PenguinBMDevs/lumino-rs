//! 主音轨增量事件（NoteDeltaEvent）专项测试
//!
//! 覆盖：事件记录（拖动/变换）、连续区间合并、dirty 对账（散改/删除/其他轨）。

use std::collections::HashSet;

use lumino_note_core::note::Note;

use super::EditorData;
use crate::editor_state::editor_data::NoteDeltaEvent;

fn make_data(note_count: usize) -> EditorData {
    let mut data = EditorData::new();
    data.current_track = 1;
    for i in 0..note_count {
        data.notes
            .push_back(Note::new((i * 10) as f32, 60 + i as u16, 1.0));
    }
    data.track_notes.insert(1, data.notes.clone());
    data
}

/// 提取事件中的 (start_index, len) 列表（断言辅助）
fn event_ranges(events: &[NoteDeltaEvent]) -> Vec<(usize, usize)> {
    events
        .iter()
        .map(|e| match e {
            NoteDeltaEvent::UpdateRange { start_index, notes } => (*start_index, notes.len()),
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
    // track_notes 同步生效
    let track = data.track_notes.get(&1).unwrap();
    assert_eq!(track[1].tick, 15.0);
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
fn test_delete_sets_dirty_full_fallback() {
    let mut data = make_data(4);
    data.delete_note_by_index(1);
    assert!(data.note_delta_dirty, "删除（增删变化）→ 全量兜底");
}

#[test]
fn test_scattered_edit_sets_dirty_full_fallback() {
    // 绕过事件 API 的散改（如 apply_note_edit 直接 get_mut）→ dirty 兜底
    let mut data = make_data(3);
    data.notes[0].tick = 99.0;
    data.mark_current_track_changed();
    assert!(data.note_delta_dirty, "散改未记录事件 → 全量兜底");
}

#[test]
fn test_other_track_mark_does_not_dirty_main_track() {
    // 洋葱皮音轨（其他轨）编辑 → 主音轨数据未变 → 不置 dirty
    let mut data = make_data(3);
    data.mark_track_notes_changed_for(Some(HashSet::from([2])));
    assert!(!data.note_delta_dirty, "其他轨变化不影响主音轨增量路径");
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
fn test_record_update_ranges_out_of_bounds_filtered() {
    let mut data = make_data(3);
    // 索引 5 越界 → 事件只含有效区间 [0, 3)
    data.record_update_ranges(&[0, 1, 2, 5, 6]);
    assert_eq!(event_ranges(&data.note_delta_events), vec![(0, 3)]);
}
