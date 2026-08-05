//! History 模块单元测试
//!
//! 覆盖：
//! - 基础 push / undo / redo / clear 行为
//! - EditorSnapshot 构造
//! - max_size 裁剪 / discard_last 不触碰 redo
//! - 元数据：group_id 分配 / op_kind / entry_count
//! - 合并窗口（push_mergeable）：窗口内合并 / 超限分割 / 不同 op_kind 不合并
//! - 逻辑撤销/重做（undo_logical / redo_logical）：单 group 退化为 undo / 跨 parent chain
//! - set_config 运行时裁剪栈
//! - MoveOp inverse / push_move_op / 混合 undo-redo

use super::*;
use std::sync::Arc;

use lumino_midi_model::NoteEvent;

fn make_snapshot(notes_len: usize) -> EditorSnapshot {
    let mut notes: Vec<NoteEvent> = Vec::with_capacity(notes_len);
    for i in 0..notes_len {
        notes.push(NoteEvent::new(i as u32, i as u32 + 1, 60, 100, 0));
    }
    EditorSnapshot::new(Arc::new(notes), 0, Vec::new())
}

fn assert_snapshot(entry: &HistoryEntry) -> &EditorSnapshot {
    match entry {
        HistoryEntry::Snapshot(bx) => &*bx,
        HistoryEntry::Operation(_) => panic!("期望 Snapshot，得到 Operation"),
    }
}

fn assert_operation(entry: &HistoryEntry) -> &OperationEntry {
    match entry {
        HistoryEntry::Snapshot(_) => panic!("期望 Operation，得到 Snapshot"),
        HistoryEntry::Operation(o) => o,
    }
}

fn op_kind_of(entry: &HistoryEntry) -> OpKind {
    match entry {
        HistoryEntry::Snapshot(bx) => bx.op_kind,
        HistoryEntry::Operation(o) => o.op_kind,
    }
}

// ── 基础测试 ──────────────────────────────────────────────────

#[test]
fn test_history_new_is_empty() {
    let history = History::new();
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn test_history_push_and_undo() {
    let mut history = History::new();
    history.push(make_snapshot(1));
    assert!(history.can_undo());
    assert_eq!(history.undo_len(), 1);

    let current = make_snapshot(2);
    let prev = history.undo(current).expect("应有快照返回");
    let prev = assert_snapshot(&prev);
    assert_eq!(prev.notes.len(), 1);
    assert!(history.can_redo());
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 1);
}

#[test]
fn test_history_undo_empty() {
    let mut history = History::new();
    let current = make_snapshot(0);
    assert!(history.undo(current).is_none());
}

#[test]
fn test_history_redo_empty() {
    let mut history = History::new();
    let current = make_snapshot(0);
    assert!(history.redo(current).is_none());
}

#[test]
fn test_history_redo_after_undo() {
    let mut history = History::new();
    history.push(make_snapshot(1));
    let _ = history.undo(make_snapshot(2));
    assert!(history.can_redo());

    let current = make_snapshot(1);
    let restored = history.redo(current).expect("redo 应返回快照");
    let restored = assert_snapshot(&restored);
    assert_eq!(restored.notes.len(), 2);
    assert!(!history.can_redo());
    assert!(history.can_undo());
}

#[test]
fn test_history_new_push_clears_redo_stack() {
    let mut history = History::new();
    history.push(make_snapshot(1));
    let _ = history.undo(make_snapshot(2));
    assert!(history.can_redo());

    // 新 push 必须清空 redo
    history.push(make_snapshot(3));
    assert!(!history.can_redo());
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn test_history_max_size() {
    let mut history = History::with_config(3, 300, 1000);
    for i in 1..=5 {
        history.push(make_snapshot(i));
    }
    assert_eq!(history.undo_len(), 3, "栈应被裁剪到 max_size");
    // 最早的 1、2 被弹出，栈顶是 5
    assert_eq!(assert_snapshot(history.undo_back().unwrap()).notes.len(), 5);
}

#[test]
fn test_discard_last_keeps_redo_stack() {
    let mut history = History::new();
    history.push(make_snapshot(1));
    history.push(make_snapshot(2));
    let _ = history.undo(make_snapshot(3));
    // 现在 undo_stack=[snap1], redo_stack=[current_with_3]
    assert_eq!(history.redo_len(), 1);

    history.discard_last();
    assert_eq!(history.undo_len(), 0, "discard_last 只 pop undo 栈顶");
    assert_eq!(
        history.redo_len(),
        1,
        "discard_last 必须保留 redo 栈（区别于 undo）"
    );
}

#[test]
fn test_history_clear() {
    let mut history = History::new();
    history.push(make_snapshot(1));
    let _ = history.undo(make_snapshot(2));
    history.clear();
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn test_history_undo_redo_roundtrip() {
    let mut history = History::new();
    history.push(make_snapshot(1));
    history.push(make_snapshot(2));

    let snap1 = history.undo(make_snapshot(3)).unwrap();
    assert_eq!(assert_snapshot(&snap1).notes.len(), 2);

    let snap2 = history.redo(make_snapshot(2)).unwrap();
    assert_eq!(assert_snapshot(&snap2).notes.len(), 3);

    // 再次 undo 应能回到 snap2
    let snap3 = history.undo(make_snapshot(3)).unwrap();
    assert_eq!(assert_snapshot(&snap3).notes.len(), 2);
}

#[test]
fn test_editor_snapshot_new() {
    let snap = make_snapshot(3);
    assert_eq!(snap.notes.len(), 3);
    assert_eq!(snap.current_track, 0);
    assert!(snap.automation_lanes.is_empty());
    assert_eq!(snap.op_kind, OpKind::Other);
    assert_eq!(snap.entry_count, 1);
    assert!(snap.group_id.is_none());
    assert!(snap.parent_group_id.is_none());
}

// ── 元数据 / op_kind 测试 ─────────────────────────────────────

#[test]
fn test_push_with_op_kind_assigns_group_id() {
    let mut history = History::new();
    history.push_with_op_kind(make_snapshot(1), OpKind::NoteDelete);

    let back = assert_snapshot(history.undo_back().unwrap());
    assert_eq!(back.op_kind, OpKind::NoteDelete);
    assert_eq!(back.entry_count, 1);
    assert!(back.parent_group_id.is_none(), "首次 push 无 parent");
    let gid = back.group_id.expect("push_with_op_kind 必须分配 group_id");

    // 第二次 push 应分配不同的 group_id
    history.push_with_op_kind(make_snapshot(2), OpKind::NoteDelete);
    let back2 = assert_snapshot(history.undo_back().unwrap());
    assert_ne!(
        back2.group_id.unwrap(),
        gid,
        "不同 push 必须分配不同 group_id"
    );
}

#[test]
fn test_op_kind_is_mergeable_only_note_create() {
    assert!(OpKind::NoteCreate.is_mergeable());
    assert!(!OpKind::NoteMove.is_mergeable());
    assert!(!OpKind::NoteDelete.is_mergeable());
    assert!(!OpKind::VelocityEdit.is_mergeable());
    assert!(!OpKind::Recording.is_mergeable());
    assert!(!OpKind::Other.is_mergeable());
    assert!(!OpKind::ChainMarker.is_mergeable());
}

#[test]
fn test_op_kind_is_chain_marker() {
    assert!(OpKind::ChainMarker.is_chain_marker());
    assert!(!OpKind::Other.is_chain_marker());
}

// ── 合并窗口测试 ─────────────────────────────────────────────

#[test]
fn test_push_mergeable_within_window_merges() {
    // 配置：合并窗口 1000ms（足够大），单条上限 1000
    let mut history = History::with_config(100, 1000, 1000);

    // 第一次 push：新增
    let merged = history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    assert!(!merged, "首次 push 不应合并");
    assert_eq!(history.undo_len(), 1);
    assert_eq!(assert_snapshot(history.undo_back().unwrap()).entry_count, 1);

    // 第二次 push：合并到栈顶，entry_count + 1，notes 保留 snap1
    let merged = history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert!(merged, "同 op_kind + 窗口内 + 未超限应合并");
    assert_eq!(history.undo_len(), 1, "合并后栈大小不变");
    assert_eq!(assert_snapshot(history.undo_back().unwrap()).entry_count, 2);

    // 第三次 push：继续合并
    history.push_mergeable(make_snapshot(3), OpKind::NoteCreate);
    assert_eq!(history.undo_len(), 1);
    assert_eq!(assert_snapshot(history.undo_back().unwrap()).entry_count, 3);
    assert_eq!(
        assert_snapshot(history.undo_back().unwrap()).notes.len(),
        1,
        "合并后快照内容应保留 chain 中最早操作之前的状态（snap1）"
    );
}

#[test]
fn test_push_mergeable_exceeds_limit_splits_with_parent() {
    // 配置：合并窗口 1000ms，单条上限 2
    let mut history = History::with_config(100, 1000, 2);

    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert_eq!(
        assert_snapshot(history.undo_back().unwrap()).entry_count,
        2,
        "应已合并到上限"
    );
    let old_group_id = assert_snapshot(history.undo_back().unwrap())
        .group_id
        .unwrap();

    // 第三次 push：应分割为新 group，parent 指向旧 group
    let merged = history.push_mergeable(make_snapshot(3), OpKind::NoteCreate);
    assert!(!merged, "超限分割不算合并");
    assert_eq!(history.undo_len(), 2, "分割后栈大小 +1");

    let top = assert_snapshot(history.undo_back().unwrap());
    assert_eq!(top.entry_count, 1, "新分割组 entry_count 重置为 1");
    assert!(
        top.parent_group_id.is_some(),
        "分割组必须有 parent_group_id 指向被分割的旧 group"
    );
    assert_eq!(
        top.parent_group_id.unwrap(),
        old_group_id,
        "parent_group_id 必须指向被分割的旧 group_id"
    );
    assert_ne!(
        top.group_id.unwrap(),
        old_group_id,
        "新分割组必须分配新 group_id"
    );
}

#[test]
fn test_push_mergeable_different_kind_no_merge() {
    let mut history = History::with_config(100, 1000, 1000);

    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    // 不同 op_kind 不应合并
    let merged = history.push_mergeable(make_snapshot(2), OpKind::NoteDelete);
    assert!(!merged, "不同 op_kind 必须不合并");
    assert_eq!(history.undo_len(), 2);
}

#[test]
fn test_push_mergeable_zero_window_never_merges() {
    // 合并窗口 = 0：任何两次连续 push 都不可能在窗口内
    let mut history = History::with_config(100, 0, 1000);

    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    let merged = history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert!(!merged, "窗口=0 时不应合并");
    assert_eq!(history.undo_len(), 2);
}

// ── 逻辑撤销/重做测试 ────────────────────────────────────────

#[test]
fn test_undo_logical_single_group_degrades_to_undo() {
    // 单条 group（无 parent chain）应退化为普通 undo
    let mut history = History::new();
    history.push_with_op_kind(make_snapshot(1), OpKind::NoteMove);

    let current = make_snapshot(2);
    let prev = history
        .undo_logical(current)
        .expect("应返回 chain 中最早的 snapshot");
    let prev = assert_snapshot(&prev);
    assert_eq!(
        prev.notes.len(),
        1,
        "prev 应为 chain 中最早操作之前的状态（snap1）"
    );
    assert_eq!(history.undo_len(), 0, "undo 后栈空");
    assert_eq!(
        history.redo_len(),
        2,
        "current(marker) + 1 个被撤销的快照进 redo"
    );
    // redo 栈底应是 ChainMarker（current），因为先 push marker 再 push chain snapshots
    assert_eq!(
        history.redo_front().map(op_kind_of),
        Some(OpKind::ChainMarker),
        "current 应被标记为 ChainMarker 推入 redo 栈底"
    );
    // redo 栈顶是被撤销的 NoteMove 快照
    assert_eq!(
        history.redo_back().map(op_kind_of),
        Some(OpKind::NoteMove),
        "redo 栈顶应是被撤销的快照"
    );
}

#[test]
fn test_undo_logical_cross_parent_chain() {
    // 配置：合并窗口 1000ms，单条上限 2 → 第 3 次 push 触发分割
    let mut history = History::with_config(100, 1000, 2);

    // chain: group1 (entry 2, snap1) → group2 (entry 1, snap3, parent=group1)
    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(3), OpKind::NoteCreate);
    assert_eq!(history.undo_len(), 2, "应有 2 个分组");

    // chain 之上 push 一条 NoteMove（独立 group，不在 chain 内）
    history.push_with_op_kind(make_snapshot(4), OpKind::NoteMove);
    assert_eq!(history.undo_len(), 3);

    // undo_logical 应只回退栈顶 NoteMove（不在 chain 内，单 group chain）
    // 返回的 prev 应为 NoteMove 之前的状态 = snap4
    let current = make_snapshot(5);
    let prev = history
        .undo_logical(current)
        .expect("应返回 NoteMove 之前的快照");
    let prev = assert_snapshot(&prev);
    assert_eq!(
        prev.notes.len(),
        4,
        "prev 应为 NoteMove 之前的状态（snap4.notes.len()=4）"
    );
    assert_eq!(history.undo_len(), 2, "NoteMove 被 pop，chain 仍在");
    assert_eq!(history.redo_len(), 2, "marker + NoteMove snap");
}

#[test]
fn test_undo_logical_chain_cross_full_chain() {
    // 完整 chain：group1 (snap1) → group2 (snap3, parent=group1)
    // undo_logical 应一次性回退整个 chain，返回 snap1（chain 最早操作之前）
    let mut history = History::with_config(100, 1000, 2);
    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(3), OpKind::NoteCreate);
    assert_eq!(history.undo_len(), 2);

    let current = make_snapshot(3);
    let prev = history.undo_logical(current).expect("应跨 chain 回退");
    let prev = assert_snapshot(&prev);

    // 整个 chain 应被一次性撤销
    assert_eq!(history.undo_len(), 0, "chain 中所有快照应被 pop 到 redo");
    assert_eq!(
        history.redo_len(),
        3,
        "current(marker) + 2 个被撤销快照（snap1 + snap3）"
    );
    assert_eq!(
        prev.notes.len(),
        1,
        "prev 应为 chain 中最早操作之前的状态（snap1.notes.len()=1）"
    );
}

#[test]
fn test_redo_logical_cross_parent_chain() {
    // 先 undo_logical，再 redo_logical 应恢复
    let mut history = History::with_config(100, 1000, 2);
    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(3), OpKind::NoteCreate);
    let before_undo_len = history.undo_len(); // 2

    let current = make_snapshot(3);
    let _ = history.undo_logical(current);

    // redo_stack 应包含 chain marker + 2 个快照（snap1 + snap3）
    assert!(history.can_redo());

    let current_for_redo = make_snapshot(1); // 撤销后的当前状态（snap1.notes）
    let restored = history
        .redo_logical(current_for_redo)
        .expect("应能重做整个 chain");
    let restored = assert_snapshot(&restored);
    assert_eq!(
        restored.notes.len(),
        3,
        "redo_logical 应恢复到 chain 的 after 状态（current.notes.len()=3）"
    );
    // redo 后 undo_stack = [current_for_redo, snap1, snap3]，长度 = before_undo_len + 1
    assert_eq!(
        history.undo_len(),
        before_undo_len + 1,
        "redo 后 undo 栈 = chain 长度 + current_for_redo"
    );
}

// ── set_config 测试 ──────────────────────────────────────────

#[test]
fn test_set_config_trims_stack() {
    let mut history = History::with_config(100, 300, 1000);
    for i in 1..=10 {
        history.push(make_snapshot(i));
    }
    assert_eq!(history.undo_len(), 10);

    // 收紧上限到 5：应立即裁剪
    history.set_config(5, 300, 1000);
    assert_eq!(history.undo_len(), 5, "set_config 应立即裁剪栈");
    // 最早的 1-5 被弹出，栈顶是 10
    assert_eq!(
        assert_snapshot(history.undo_back().unwrap()).notes.len(),
        10
    );
}

#[test]
fn test_set_config_updates_merge_params() {
    let mut history = History::with_config(100, 1000, 1000);
    history.set_config(100, 0, 1);
    // 窗口=0 + 单条上限=1：不可能合并，第二次 push 必然分割
    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert_eq!(history.undo_len(), 2, "窗口=0 + 上限=1 时每次 push 都新增");
}

// ── MoveOp 测试 ──────────────────────────────────────────────

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
