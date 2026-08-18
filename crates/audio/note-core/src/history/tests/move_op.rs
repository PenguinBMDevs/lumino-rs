//! MoveOp 操作日志测试
//!
//! 覆盖：
//! - MoveOp inverse（含双重取反、i32::MIN 回绕）
//! - push_move_op 创建 Operation 条目
//! - undo/redo MoveOp roundtrip / 多操作序列 / 混合快照与操作

use super::{assert_operation, assert_snapshot, make_snapshot};
use crate::history::{History, HistoryEntry, MoveOp, OpKind};

#[test]
fn test_move_op_inverse() {
    let move_op = MoveOp {
        track_id: 1,
        range_start: 10,
        range_end: 20,
        delta_tick: 100,
        delta_key: -5,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    };
    let inv = move_op.inverse();
    assert_eq!(inv.track_id, move_op.track_id);
    assert_eq!(inv.range_start, move_op.range_start);
    assert_eq!(inv.range_end, move_op.range_end);
    assert_eq!(inv.delta_tick, -100);
    assert_eq!(inv.delta_key, 5);
    assert_eq!(inv.seq, move_op.seq);

    // 双重取反应等于原操作
    let inv_inv = inv.inverse();
    assert_eq!(inv_inv, move_op);
}

#[test]
fn test_push_move_op_creates_operation_entry() {
    let mut history = History::new();
    let ops = vec![MoveOp {
        track_id: 0,
        range_start: 0,
        range_end: 3,
        delta_tick: 10,
        delta_key: 2,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    let gid = history.push_move_op(ops.clone());
    assert!(gid > 0);

    let back = history.undo_back().expect("undo 栈顶应存在");
    let op_entry = assert_operation(back);
    assert_eq!(op_entry.op_kind, OpKind::NoteMove);
    assert_eq!(op_entry.ops.len(), 1);
    assert_eq!(op_entry.ops[0].delta_tick, 10);
    assert_eq!(op_entry.group_id, Some(gid));
}

#[test]
fn test_undo_redo_move_op_roundtrip() {
    let mut history = History::new();
    let ops = vec![MoveOp {
        track_id: 0,
        range_start: 0,
        range_end: 2,
        delta_tick: 5,
        delta_key: -1,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    history.push_move_op(ops);

    let current = make_snapshot(2);
    let entry = history
        .undo(current)
        .expect("undo 应返回 inverse Operation");
    let op_entry = assert_operation(&entry);
    assert_eq!(op_entry.ops[0].delta_tick, -5);
    assert_eq!(op_entry.ops[0].delta_key, 1);
    assert_eq!(
        history.redo_len(),
        1,
        "Operation undo 只应推入一个反向 Operation"
    );
    assert_eq!(history.undo_len(), 0);

    // redo 应恢复为 forward op
    let current_for_redo = make_snapshot(1);
    let redo_entry = history
        .redo(current_for_redo)
        .expect("redo 应返回 forward Operation");
    let redo_op = assert_operation(&redo_entry);
    assert_eq!(redo_op.ops[0].delta_tick, 5);
    assert_eq!(redo_op.ops[0].delta_key, -1);
    assert_eq!(
        history.undo_len(),
        1,
        "Operation redo 只应推入一个正向 Operation"
    );
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn test_multiple_move_op_undo_redo_sequence() {
    let mut history = History::new();
    let ops1 = vec![MoveOp {
        track_id: 0,
        range_start: 0,
        range_end: 1,
        delta_tick: 100,
        delta_key: 5,
        seq: 0,
        original_ticks: vec![0.0],
        original_keys: vec![60],
    }];
    let ops2 = vec![MoveOp {
        track_id: 0,
        range_start: 0,
        range_end: 1,
        delta_tick: 50,
        delta_key: 3,
        seq: 0,
        original_ticks: vec![100.0],
        original_keys: vec![65],
    }];
    history.push_move_op(ops1);
    history.push_move_op(ops2);

    // undo 第二个操作：返回 inverse，delta = -50
    let entry = history.undo(make_snapshot(2)).expect("第一次 undo");
    let operation = assert_operation(&entry);
    assert_eq!(operation.ops[0].delta_tick, -50);

    // undo 第一个操作：返回 inverse，delta = -100
    let entry = history.undo(make_snapshot(1)).expect("第二次 undo");
    let operation = assert_operation(&entry);
    assert_eq!(operation.ops[0].delta_tick, -100);

    // redo 第一个操作：delta = 100
    let entry = history.redo(make_snapshot(0)).expect("第一次 redo");
    let operation = assert_operation(&entry);
    assert_eq!(operation.ops[0].delta_tick, 100);

    // redo 第二个操作：delta = 50
    let entry = history.redo(make_snapshot(1)).expect("第二次 redo");
    let operation = assert_operation(&entry);
    assert_eq!(operation.ops[0].delta_tick, 50);

    assert_eq!(history.undo_len(), 2);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn test_mixed_snapshot_and_operation_undo_order() {
    let mut history = History::new();
    // 先 push 一个快照
    history.push(make_snapshot(1));
    // 再 push 一个 MoveOp
    let ops = vec![MoveOp {
        track_id: 0,
        range_start: 0,
        range_end: 1,
        delta_tick: 10,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    history.push_move_op(ops);

    // undo 应先返回 MoveOp 的 inverse
    let current = make_snapshot(2);
    let first_undo = history
        .undo(current)
        .expect("第一次 undo 应返回 MoveOp inverse");
    assert!(matches!(first_undo, HistoryEntry::Operation(_)));
    let first_op = assert_operation(&first_undo);
    assert_eq!(first_op.ops[0].delta_tick, -10);

    // 再次 undo 返回快照
    let current2 = make_snapshot(1);
    let second_undo = history.undo(current2).expect("第二次 undo 应返回 Snapshot");
    assert!(matches!(second_undo, HistoryEntry::Snapshot(_)));
    let snap = assert_snapshot(&second_undo);
    assert_eq!(snap.notes.len(), 1);
}

#[test]
fn test_logical_undo_operation_degrades_to_single() {
    let mut history = History::new();
    let ops = vec![MoveOp {
        track_id: 0,
        range_start: 0,
        range_end: 1,
        delta_tick: 7,
        delta_key: 3,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    history.push_move_op(ops);

    let current = make_snapshot(2);
    let entry = history
        .undo_logical(current)
        .expect("Operation 逻辑 undo 退化为单步");
    let op_entry = assert_operation(&entry);
    assert_eq!(op_entry.ops[0].delta_tick, -7);
    assert_eq!(op_entry.ops[0].delta_key, -3);
    assert_eq!(history.undo_len(), 0);
    assert_eq!(
        history.redo_len(),
        1,
        "Operation 逻辑 undo 同样只推入一个反向 Operation"
    );
}

#[test]
fn test_move_op_inverse_with_i32_min() {
    let move_op = MoveOp {
        track_id: 0,
        range_start: 0,
        range_end: 1,
        delta_tick: i32::MIN,
        delta_key: i16::MIN,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    };
    let inv = move_op.inverse();
    // wrapping_neg(i32::MIN) == i32::MIN
    assert_eq!(inv.delta_tick, i32::MIN);
    assert_eq!(inv.delta_key, i16::MIN);
}
