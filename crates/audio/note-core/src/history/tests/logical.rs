//! 逻辑撤销/重做（undo_logical / redo_logical）测试
//!
//! 覆盖：
//! - 单 group 退化为普通 undo（ChainMarker 进入 redo 栈底）
//! - 跨 parent chain 一次性回退/重做整个逻辑操作

use super::{assert_snapshot, make_snapshot, op_kind_of};
use crate::history::{History, OpKind};

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
