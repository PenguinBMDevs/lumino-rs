//! 异步批量提交的 CPU 洋葱皮密度增量（`tests.rs` 子模块）。

use super::super::{EditorData, MAX_CPU_ONION_CHANGES, move_op_cpu_onion_changes};
use super::make_data_with_notes;
use crate::DragState;
use bit_vec::BitVec;
use lumino_note_core::history::MoveOp;
use lumino_note_core::note::Note;

#[test]
fn move_op_changes_are_bounded_and_track_checked() {
    let op = MoveOp {
        track_id: 0,
        originals: vec![lumino_midi_model::NoteEvent::new(0, 10, 60, 100, 0)],
        moved: vec![lumino_midi_model::NoteEvent::new(5, 15, 60, 100, 0)],
        delta_tick: 5,
        delta_key: 0,
        seq: 0,
    };
    let changes = move_op_cpu_onion_changes(&[op], 0).expect("小批量必须提炼成功");
    assert_eq!(changes.len(), 2, "删旧 + 加新各一条");
    assert_eq!(changes[0].old.expect("old").start_tick, 0);
    assert_eq!(changes[1].new.expect("new").start_tick, 5);

    let big = MoveOp {
        track_id: 0,
        originals: (0..(MAX_CPU_ONION_CHANGES / 2 + 2))
            .map(|i| lumino_midi_model::NoteEvent::new(i as u32, i as u32 + 1, 60, 100, 0))
            .collect(),
        moved: (0..(MAX_CPU_ONION_CHANGES / 2 + 2))
            .map(|i| lumino_midi_model::NoteEvent::new(i as u32 + 5, i as u32 + 6, 60, 100, 0))
            .collect(),
        delta_tick: 5,
        delta_key: 0,
        seq: 0,
    };
    assert!(
        move_op_cpu_onion_changes(&[big], 0).is_none(),
        "超过变更上限必须整帧兜底"
    );
    let cross = MoveOp {
        track_id: 1,
        originals: vec![lumino_midi_model::NoteEvent::new(0, 1, 60, 100, 0)],
        moved: vec![lumino_midi_model::NoteEvent::new(5, 6, 60, 100, 0)],
        delta_tick: 5,
        delta_key: 0,
        seq: 0,
    };
    assert!(
        move_op_cpu_onion_changes(&[cross], 0).is_none(),
        "跨轨 op 不得按单轨替换走增量"
    );
}

#[test]
fn async_batch_move_records_incremental_density_changes() {
    let mut data = make_data_with_notes();
    data.set_cpu_onion_dirty_enabled(true);
    let mut bv = BitVec::from_elem(3, false);
    bv.set(0, true);
    bv.set(2, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(5, -2);
    let ops = data.move_ops_from_drag_state(&ds);
    assert!(data.apply_move_ops_async(ops, 127).expect("异步提交应成功"));
    loop {
        if let Some(result) = data.poll_async_commit() {
            result.expect("异步提交应成功");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let dirty = data.cpu_onion_dirty().expect("批量移动必须记录 CPU 脏区");
    assert!(!dirty.full, "≤上限的批量移动应走密度增量");
    assert_eq!(dirty.changes.len(), 4, "2 个音符 ×（删旧 + 加新）");
}

#[test]
fn large_async_batch_move_uses_dirty_range() {
    let notes: Vec<Note> = (0..20)
        .map(|i| Note::new(i as f32 * 10.0, 60, 1.0))
        .collect();
    let mut data = EditorData::with_f32_notes(1, &notes);
    data.set_cpu_onion_dirty_enabled(true);
    let bv = BitVec::from_elem(20, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(5, 0);
    let ops = data.move_ops_from_drag_state(&ds);
    assert!(data.apply_move_ops_async(ops, 127).expect("异步提交应成功"));
    loop {
        if let Some(result) = data.poll_async_commit() {
            result.expect("异步提交应成功");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let dirty = data.cpu_onion_dirty().expect("大批量必须有记录");
    assert!(!dirty.full, "超过上限的批量不得整帧兜底");
    assert!(!dirty.ranges.is_empty(), "必须转成脏 tick 区间");
}
