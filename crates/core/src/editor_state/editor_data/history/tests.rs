//! EditorData 历史记录与 MoveOp 单元测试
//!
//! 覆盖：
//! - `apply_move_ops` 正向/反向应用
//! - key clamp、缺失 track_notes 自动创建
//! - `move_ops_from_drag_state` 连续区间拆分、delta 饱和
//! - 基于 MoveOp 的 undo/redo 往返

use super::*;
use crate::note::Note;
use bit_vec::BitVec;

fn make_data_with_notes() -> EditorData {
    let mut data = EditorData::new();
    data.current_track = 1;
    data.notes.push_back(Note::new(0.0, 60, 1.0));
    data.notes.push_back(Note::new(10.0, 62, 1.0));
    data.notes.push_back(Note::new(20.0, 64, 1.0));
    data.track_notes.insert(1, data.notes.clone());
    data
}

#[test]
fn test_apply_move_ops_forward() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 2,
        delta_tick: 5,
        delta_key: -2,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    let modified = data.apply_move_ops(&ops, false, 127);
    assert_eq!(modified, 2);
    assert_eq!(data.notes[0].tick, 5.0);
    assert_eq!(data.notes[0].key, 58);
    assert_eq!(data.notes[1].tick, 15.0);
    assert_eq!(data.notes[1].key, 60);
    assert_eq!(data.notes[2].tick, 20.0, "未在范围内的音符不变");
    assert_eq!(data.notes[2].key, 64);

    // track_notes 同步更新
    let track = data.track_notes.get(&1).expect("track 1 应存在");
    assert_eq!(track[0].tick, 5.0);
    assert_eq!(track[1].tick, 15.0);
}

#[test]
fn test_apply_move_ops_inverse() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 3,
        delta_tick: 10,
        delta_key: 5,
        seq: 0,
        original_ticks: vec![0.0, 10.0, 20.0],
        original_keys: vec![60, 62, 64],
    }];
    // 先 forward
    data.apply_move_ops(&ops, false, 127);
    // 再 inverse 应还原
    let modified = data.apply_move_ops(&ops, true, 127);
    assert_eq!(modified, 3);
    assert_eq!(data.notes[0].tick, 0.0);
    assert_eq!(data.notes[0].key, 60);
    assert_eq!(data.notes[1].tick, 10.0);
    assert_eq!(data.notes[1].key, 62);
    assert_eq!(data.notes[2].tick, 20.0);
    assert_eq!(data.notes[2].key, 64);
}

#[test]
fn test_apply_move_ops_clamps_key() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 1,
        delta_tick: 0,
        delta_key: -100,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops, false, 20);
    assert_eq!(data.notes[0].key, 0, "key 应 clamp 到 0");

    let ops2 = vec![MoveOp {
        track_id: 1,
        range_start: 1,
        range_end: 2,
        delta_tick: 0,
        delta_key: 100,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops2, false, 20);
    assert_eq!(data.notes[1].key, 20, "key 应 clamp 到 max_key");
}

#[test]
fn test_apply_move_ops_creates_missing_track_notes() {
    let mut data = EditorData::new();
    data.current_track = 2;
    data.notes.push_back(Note::new(0.0, 60, 1.0));
    // track_notes 中无 track 2
    let ops = vec![MoveOp {
        track_id: 2,
        range_start: 0,
        range_end: 1,
        delta_tick: 3,
        delta_key: 1,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops, false, 127);
    assert!(data.track_notes.contains_key(&2));
    assert_eq!(data.track_notes[&2][0].tick, 3.0);
    assert_eq!(data.track_notes[&2][0].key, 61);
}

#[test]
fn test_move_ops_from_drag_state_splits_ranges() {
    let mut data = EditorData::new();
    data.current_track = 1;
    data.notes.push_back(Note::new(0.0, 60, 1.0));
    data.notes.push_back(Note::new(10.0, 62, 1.0));
    data.notes.push_back(Note::new(20.0, 64, 1.0));
    data.notes.push_back(Note::new(30.0, 66, 1.0));

    let mut bv = BitVec::from_elem(4, false);
    bv.set(0, true);
    bv.set(1, true);
    bv.set(3, true);
    let mut drag_state = DragState::new(bv, 0, 60);
    drag_state.set_delta(5, -2);

    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops.len(), 2, "应拆分为两个连续段");
    assert_eq!(ops[0].range_start, 0);
    assert_eq!(ops[0].range_end, 2);
    assert_eq!(ops[0].delta_tick, 5);
    assert_eq!(ops[0].delta_key, -2);
    assert_eq!(ops[0].seq, 0);

    assert_eq!(ops[1].range_start, 3);
    assert_eq!(ops[1].range_end, 4);
    assert_eq!(ops[1].delta_tick, 5);
    assert_eq!(ops[1].delta_key, -2);
    assert_eq!(ops[1].seq, 1);
}

#[test]
fn test_move_ops_from_drag_state_saturates_delta_tick() {
    let mut data = EditorData::new();
    data.current_track = 1;
    data.notes.push_back(Note::new(0.0, 60, 1.0));

    let mut drag_state = DragState::from_single(0, 1, 0, 60);
    drag_state.set_delta(i64::MAX, 0);

    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops[0].delta_tick, i32::MAX, "delta_tick 应饱和到 i32::MAX");

    drag_state.set_delta(i64::MIN, 0);
    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops[0].delta_tick, i32::MIN, "delta_tick 应饱和到 i32::MIN");
}

#[test]
fn test_undo_redo_with_move_op_entry() {
    let mut data = make_data_with_notes();
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2);
        ds
    });
    data.apply_move_ops(&ops, false, 127);
    data.push_move_op(ops);

    // undo 应还原
    assert!(data.undo());
    assert_eq!(data.notes[0].tick, 0.0);
    assert_eq!(data.notes[0].key, 60);
    assert_eq!(data.notes[2].tick, 20.0);
    assert_eq!(data.notes[2].key, 64);

    // redo 应再次应用
    assert!(data.redo());
    assert_eq!(data.notes[0].tick, 5.0);
    assert_eq!(data.notes[0].key, 58);
    assert_eq!(data.notes[2].tick, 25.0);
    assert_eq!(data.notes[2].key, 62);
}
