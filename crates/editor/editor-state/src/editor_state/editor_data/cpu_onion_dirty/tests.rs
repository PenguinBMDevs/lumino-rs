//! CPU 洋葱皮脏区元数据验收：记录点、full 升级、消费语义与 GPU 默认零开销。

use lumino_midi_model::NoteEvent;
use lumino_note_core::note::Note;

use super::super::EditorData;
use super::{CpuOnionDirty, MAX_CPU_ONION_CHANGES, MAX_CPU_ONION_RANGES};

/// 两音符、单轨（当前轨 = 0）的编辑器数据。
fn data() -> EditorData {
    EditorData::with_notes(
        0,
        &[
            NoteEvent::new(0, 100, 60, 100, 0),
            NoteEvent::new(200, 300, 62, 100, 0),
        ],
    )
}

/// 默认（GPU 后端）**零记录零分配**：单笔编辑不产生任何脏区。
#[test]
fn disabled_by_default_records_nothing() {
    let mut data = data();
    assert!(
        !data.cpu_onion_dirty_enabled(),
        "默认必须关闭（GPU 零开销）"
    );

    let _ = data.insert_note_with_id(0, Note::new(400.0, 64, 100.0));
    let _ = data.remove_note(0, 0);
    let _ = data.update_note(0, 0, Note::new(500.0, 65, 50.0));

    assert!(
        data.cpu_onion_dirty().is_none(),
        "关闭状态下任何编辑都不得产生脏区"
    );
    assert!(data.take_cpu_onion_dirty().is_none());
}

/// 开启后：插入记录 `(track, None, Some(new))`。
#[test]
fn insert_records_new_note() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);

    assert!(
        data.insert_note_with_id(0, Note::new(400.0, 64, 100.0))
            .is_some()
    );
    let dirty = data.take_cpu_onion_dirty().expect("插入必须记录脏区");
    assert!(!dirty.full, "单笔插入不得升级为整帧");
    assert_eq!(dirty.changes.len(), 1);
    let change = dirty.changes[0];
    assert_eq!(change.track, 0);
    assert!(change.old.is_none(), "插入的 old 必须为 None");
    let added = change.new.expect("插入必须带 new 音符");
    assert_eq!((added.start_tick, added.end_tick), (400, 500));
    assert_eq!(added.key, 64);
}

/// 开启后：删除记录 `(track, Some(removed), None)`。
#[test]
fn remove_records_removed_note() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);

    let removed = data.remove_note(0, 0).expect("索引 0 必须存在");
    let dirty = data.take_cpu_onion_dirty().expect("删除必须记录脏区");
    assert_eq!(dirty.changes.len(), 1);
    let change = dirty.changes[0];
    assert_eq!(change.track, 0);
    assert_eq!(change.old, Some(removed), "old 必须是被删除的音符");
    assert!(change.new.is_none(), "删除的 new 必须为 None");
}

/// 开启后：更新记录 `(track, Some(old), Some(new))`，且 old 是修改前的值。
#[test]
fn update_records_before_and_after() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);

    assert!(data.update_note(0, 1, Note::new(250.0, 62, 300.0)));
    let dirty = data.take_cpu_onion_dirty().expect("更新必须记录脏区");
    assert_eq!(dirty.changes.len(), 1);
    let change = dirty.changes[0];
    assert_eq!(change.old.map(|note| note.start_tick), Some(200));
    let new = change.new.expect("更新的 new 必须存在");
    assert_eq!((new.start_tick, new.end_tick), (250, 550));
}

/// 越界更新失败时**不得**留下脏区（避免渲染层为未发生的变更重绘）。
#[test]
fn failed_update_records_nothing() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);
    assert!(!data.update_note(0, 99, Note::new(250.0, 62, 300.0)));
    assert!(data.take_cpu_onion_dirty().is_none());
}

/// 批量插入 / 整轨替换升级整帧；小批量区间删除保留按值 old。
#[test]
fn bulk_edits_escalate_to_full() {
    let mut batch = data();
    batch.set_cpu_onion_dirty_enabled(true);
    let inserted = batch.batch_insert_notes_with_ids(&[Note::new(600.0, 70, 40.0)]);
    assert_eq!(inserted.len(), 1);
    let dirty = batch.take_cpu_onion_dirty().expect("批量插入必须记录");
    assert!(!dirty.full, "批量插入应转脏区间，不再整帧兜底");
    assert!(!dirty.ranges.is_empty(), "批量插入必须留下 tick 区间");

    let mut replace = data();
    replace.set_cpu_onion_dirty_enabled(true);
    assert!(replace.replace_track_notes(0, vec![NoteEvent::new(0, 10, 60, 100, 0)]));
    let replaced = replace.take_cpu_onion_dirty().expect("整轨替换必须记录");
    assert!(!replaced.full, "整轨替换应转脏区间");
    assert!(!replaced.ranges.is_empty(), "整轨替换必须留下 tick 区间");

    let mut ranges = data();
    ranges.set_cpu_onion_dirty_enabled(true);
    assert!(ranges.remove_ranges_merged(0, &[(0, 1)]) > 0);
    let dirty = ranges.take_cpu_onion_dirty().expect("区间删除必须记录");
    assert!(!dirty.full, "小批量区间删除不应升级整帧");
    assert_eq!(dirty.changes.len(), 1, "必须保留被删音符的按值 old");
    assert!(dirty.changes[0].old.is_some());

    let mut merged = data();
    merged.set_cpu_onion_dirty_enabled(true);
    assert_eq!(merged.remove_notes_merged(0, &[0]), 1);
    assert!(!merged.take_cpu_onion_dirty().expect("必须有脏区").full);
}

/// 单笔插入的 undo/redo 必须回放按值变更，而不是整帧兜底。
#[test]
fn undo_redo_replays_single_note_as_changes() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);
    data.push_history();
    assert!(data.insert_note(0, Note::new(400.0, 64, 100.0)));
    assert!(data.take_cpu_onion_dirty().is_some());

    assert!(data.undo(), "插入后应可撤销");
    let undone = data.take_cpu_onion_dirty().expect("undo 必须产生脏区");
    assert!(!undone.full, "单笔 undo 不得整帧兜底");
    assert_eq!(undone.changes.len(), 1);
    assert!(undone.changes[0].old.is_some() && undone.changes[0].new.is_none());

    assert!(data.redo(), "撤销后应可重做");
    let redone = data.take_cpu_onion_dirty().expect("redo 必须产生脏区");
    assert!(!redone.full, "单笔 redo 不得整帧兜底");
    assert_eq!(redone.changes.len(), 1);
    assert!(redone.changes[0].old.is_none() && redone.changes[0].new.is_some());
}

/// 脏区间列表必须有上限；超限折叠为包围区间，仍然不整建。
#[test]
fn dirty_ranges_are_bounded() {
    let mut dirty = CpuOnionDirty::default();
    for index in 0..(MAX_CPU_ONION_RANGES + 64) {
        let base = index as u32 * 10;
        dirty.push_range(base, base + 1);
    }
    assert!(!dirty.full, "区间超限不得升级整帧");
    assert!(
        dirty.ranges.len() <= MAX_CPU_ONION_RANGES,
        "脏区间列表必须有界"
    );
}

/// 直接 `track_notes_mut` 编辑的 undo 必须转成脏 tick 区间，不得整建。
#[test]
fn direct_edit_undo_uses_dirty_ranges() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);
    data.push_history();
    if let Some(note) = data
        .document
        .as_mut()
        .and_then(|document| document.track_notes_mut(0))
        .and_then(|track| track.get_mut(0))
    {
        note.end_tick = 150;
    }
    data.record_update_ranges(&[0]);
    assert!(data.undo(), "直接编辑后应可撤销");
    let dirty = data.take_cpu_onion_dirty().expect("undo 必须产生脏区");
    assert!(!dirty.full, "直接编辑撤销不得整帧兜底");
    assert!(!dirty.ranges.is_empty(), "必须转成脏 tick 区间供局部重建");
}

/// 超过逐音符上限的区间删除转成脏区间，不整帧兜底。
#[test]
fn huge_range_delete_uses_dirty_range() {
    let mut data = EditorData::with_notes(
        0,
        &(0..=MAX_CPU_ONION_CHANGES)
            .map(|index| NoteEvent::new(index as u32 * 10, index as u32 * 10 + 5, 60, 100, 0))
            .collect::<Vec<_>>(),
    );
    data.set_cpu_onion_dirty_enabled(true);
    assert!(data.remove_ranges_merged(0, &[(0, MAX_CPU_ONION_CHANGES + 1)]) > 0);
    let dirty = data.take_cpu_onion_dirty().expect("批量删除必须记录");
    assert!(!dirty.full, "大批量删除不得整帧兜底");
    assert!(!dirty.ranges.is_empty(), "必须留下可局部重建的 tick 区间");
}

/// full 优先：已有原子变更时升级 full 会清空 changes，之后的单笔变更被忽略。
#[test]
fn full_takes_priority_and_absorbs_changes() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);
    let _ = data.insert_note_with_id(0, Note::new(400.0, 64, 100.0));
    data.record_cpu_onion_full();
    let _ = data.insert_note_with_id(0, Note::new(500.0, 65, 100.0));

    let dirty = data.take_cpu_onion_dirty().expect("必须有脏区");
    assert!(dirty.full);
    assert!(
        dirty.changes.is_empty(),
        "full 后不得残留或追加原子变更（full 已覆盖一切）"
    );
}

/// 原子变更条数超上限转成脏区间，不再升级 full。
#[test]
fn too_many_changes_use_dirty_range() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);
    let note = NoteEvent::new(0, 10, 60, 100, 0);
    for _ in 0..=MAX_CPU_ONION_CHANGES {
        data.record_cpu_onion_change(1, None, Some(note));
    }
    let dirty = data.take_cpu_onion_dirty().expect("必须有脏区");
    assert!(!dirty.full, "超过逐音符上限不得整帧兜底");
    assert!(!dirty.ranges.is_empty(), "必须转成脏 tick 区间");
}

/// `take` 消费即清空；`clear` 直接丢弃；关闭开关会清空残留。
#[test]
fn take_and_clear_empty_the_queue() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);
    let _ = data.insert_note_with_id(0, Note::new(400.0, 64, 100.0));
    assert!(data.take_cpu_onion_dirty().is_some());
    assert!(data.cpu_onion_dirty().is_none(), "take 后必须清空");
    assert!(
        data.take_cpu_onion_dirty().is_none(),
        "二次 take 必须为 None"
    );

    let _ = data.insert_note_with_id(0, Note::new(500.0, 65, 100.0));
    data.clear_cpu_onion_dirty();
    assert!(data.cpu_onion_dirty().is_none(), "clear 后必须清空");

    let _ = data.insert_note_with_id(0, Note::new(600.0, 66, 100.0));
    data.set_cpu_onion_dirty_enabled(false);
    assert!(
        data.cpu_onion_dirty().is_none(),
        "切回 GPU 关闭开关时必须清空残留脏区"
    );
}

/// `reset` 清空脏区（文档销毁后不应留任何可被误消费的变化）。
#[test]
fn reset_clears_dirty() {
    let mut data = data();
    data.set_cpu_onion_dirty_enabled(true);
    let _ = data.insert_note_with_id(0, Note::new(400.0, 64, 100.0));
    assert!(data.cpu_onion_dirty().is_some());
    data.reset();
    assert!(data.cpu_onion_dirty().is_none(), "reset 必须清空脏区");
}

/// 空脏区语义：`has_changes` / `is_empty` 对 full / changes / 空三态自洽。
#[test]
fn dirty_flags_are_consistent() {
    let empty = CpuOnionDirty::default();
    assert!(empty.is_empty());
    assert!(!empty.has_changes());

    let changes = CpuOnionDirty {
        full: false,
        changes: vec![super::CpuOnionChange {
            track: 1,
            old: None,
            new: Some(NoteEvent::new(0, 10, 60, 100, 0)),
        }],
        ranges: Vec::new(),
    };
    assert!(!changes.is_empty());
    assert!(changes.has_changes());

    let full = CpuOnionDirty {
        full: true,
        changes: Vec::new(),
        ranges: Vec::new(),
    };
    assert!(!full.is_empty());
    assert!(!full.has_changes(), "full 不算原子变更");
}
