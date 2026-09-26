//! MoveOp 操作日志测试（去 ID 按值语义）
//!
//! 覆盖：
//! - MoveOp inverse（恒等克隆：方向由 inverse 标志决定，op 本体保持前向不变）
//! - push_move_op 创建 Operation 条目
//! - undo/redo MoveOp roundtrip / 多操作序列 / 混合快照与操作

use super::{assert_operation, assert_snapshot, make_snapshot};
use crate::history::{History, HistoryEntry, MoveOp, OpKind};
use lumino_midi_model::NoteEvent;

fn ev(start: u32, key: u8) -> NoteEvent {
    NoteEvent::new(start, start + 480, key, 100, 0)
}

/// 按生产侧同一逻辑计算移动后快照（original + delta，clamp + 长度不变）。
fn moved_of(
    originals: &[NoteEvent],
    delta_tick: i32,
    delta_key: i16,
    max_key: u16,
) -> Vec<NoteEvent> {
    originals
        .iter()
        .map(|n| {
            let mut m = *n;
            let new_tick = (n.start_tick as i64 + delta_tick as i64).max(0) as u32;
            let new_key = (n.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u8;
            let len = n.end_tick.saturating_sub(n.start_tick).max(1);
            m.start_tick = new_tick;
            m.end_tick = new_tick.saturating_add(len);
            m.key = new_key;
            m
        })
        .collect()
}

#[test]
fn test_move_op_inverse() {
    let originals = vec![ev(0, 60), ev(10, 62), ev(20, 64)];
    let move_op = MoveOp {
        track_id: 1,
        moved: moved_of(&originals, 100, -5, 127),
        originals,
        delta_tick: 100,
        delta_key: -5,
        seq: 0,
    };
    let inv = move_op.inverse();
    assert_eq!(inv.track_id, move_op.track_id);
    assert_eq!(
        inv.originals, move_op.originals,
        "inverse 必须保持 originals 不变"
    );
    assert_eq!(inv.moved, move_op.moved, "inverse 必须保持 moved 不变");
    assert_eq!(
        inv.delta_tick, 100,
        "inverse 保持前向 delta（方向由标志决定）"
    );
    assert_eq!(inv.delta_key, -5);
    assert_eq!(inv.seq, move_op.seq);

    // 双重取反应等于原操作
    let inv_inv = inv.inverse();
    assert_eq!(inv_inv, move_op);
}

#[test]
fn test_push_move_op_creates_operation_entry() {
    let mut history = History::new();
    let originals = vec![ev(0, 60), ev(10, 62), ev(20, 64)];
    let ops = vec![MoveOp {
        track_id: 0,
        moved: moved_of(&originals, 10, 2, 127),
        originals,
        delta_tick: 10,
        delta_key: 2,
        seq: 0,
    }];
    let gid = history.push_move_op(ops.clone());
    assert!(gid > 0);

    let back = history.undo_back().expect("undo 栈顶应存在");
    let op_entry = assert_operation(back);
    assert_eq!(op_entry.op_kind, OpKind::NoteMove);
    assert_eq!(op_entry.ops.len(), 1);
    assert_eq!(op_entry.ops[0].delta_tick, 10);
    assert_eq!(op_entry.ops[0].originals.len(), 3);
    assert_eq!(op_entry.group_id, Some(gid));
}

#[test]
fn test_undo_redo_move_op_roundtrip() {
    let mut history = History::new();
    let originals = vec![ev(0, 60), ev(10, 62)];
    let ops = vec![MoveOp {
        track_id: 0,
        moved: moved_of(&originals, 5, -1, 127),
        originals,
        delta_tick: 5,
        delta_key: -1,
        seq: 0,
    }];
    history.push_move_op(ops);

    let current = make_snapshot(2);
    let entry = history
        .undo(current)
        .expect("undo 应返回 inverse Operation");
    let op_entry = assert_operation(&entry);
    assert_eq!(
        op_entry.ops[0].delta_tick, 5,
        "undo 返回恒等 op（方向由标志决定）"
    );
    assert_eq!(op_entry.ops[0].delta_key, -1);
    assert_eq!(
        op_entry.ops[0].originals.len(),
        2,
        "undo 后 originals 保持不变"
    );
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
        redo_op.ops[0].originals.len(),
        2,
        "redo 后 originals 保持不变"
    );
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
    let o1 = vec![ev(0, 60)];
    let ops1 = vec![MoveOp {
        track_id: 0,
        moved: moved_of(&o1, 100, 5, 127),
        originals: o1,
        delta_tick: 100,
        delta_key: 5,
        seq: 0,
    }];
    let o2 = vec![ev(100, 65)];
    let ops2 = vec![MoveOp {
        track_id: 0,
        moved: moved_of(&o2, 50, 3, 127),
        originals: o2,
        delta_tick: 50,
        delta_key: 3,
        seq: 0,
    }];
    history.push_move_op(ops1);
    history.push_move_op(ops2);

    // undo 第二个操作：返回恒等 op，delta = 50
    let entry = history.undo(make_snapshot(2)).expect("第一次 undo");
    let operation = assert_operation(&entry);
    assert_eq!(operation.ops[0].delta_tick, 50);

    // undo 第一个操作：返回恒等 op，delta = 100
    let entry = history.undo(make_snapshot(1)).expect("第二次 undo");
    let operation = assert_operation(&entry);
    assert_eq!(operation.ops[0].delta_tick, 100);

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
    let o = vec![ev(0, 60)];
    let ops = vec![MoveOp {
        track_id: 0,
        moved: moved_of(&o, 10, 0, 127),
        originals: o,
        delta_tick: 10,
        delta_key: 0,
        seq: 0,
    }];
    history.push_move_op(ops);

    // undo 应先返回 MoveOp 的 inverse（恒等）
    let current = make_snapshot(2);
    let first_undo = history
        .undo(current)
        .expect("第一次 undo 应返回 MoveOp inverse");
    assert!(matches!(first_undo, HistoryEntry::Operation(_)));
    let first_op = assert_operation(&first_undo);
    assert_eq!(first_op.ops[0].delta_tick, 10);

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
    let o = vec![ev(0, 60)];
    let ops = vec![MoveOp {
        track_id: 0,
        moved: moved_of(&o, 7, 3, 127),
        originals: o,
        delta_tick: 7,
        delta_key: 3,
        seq: 0,
    }];
    history.push_move_op(ops);

    let current = make_snapshot(2);
    let entry = history
        .undo_logical(current)
        .expect("Operation 逻辑 undo 退化为单步");
    let op_entry = assert_operation(&entry);
    assert_eq!(op_entry.ops[0].delta_tick, 7, "逻辑 undo 返回恒等 op");
    assert_eq!(op_entry.ops[0].delta_key, 3);
    assert_eq!(history.undo_len(), 0);
    assert_eq!(
        history.redo_len(),
        1,
        "Operation 逻辑 undo 同样只推入一个反向 Operation"
    );
}

#[test]
fn test_move_op_inverse_with_i32_min() {
    let o = vec![ev(0, 60)];
    let move_op = MoveOp {
        track_id: 0,
        moved: moved_of(&o, i32::MIN, i16::MIN, 127),
        originals: o,
        delta_tick: i32::MIN,
        delta_key: i16::MIN,
        seq: 0,
    };
    let inv = move_op.inverse();
    // 恒等克隆：极值保持不变（不再取反，无回绕问题）
    assert_eq!(inv.delta_tick, i32::MIN);
    assert_eq!(inv.delta_key, i16::MIN);
    assert_eq!(inv.originals, move_op.originals);
    assert_eq!(inv.moved, move_op.moved);
}
