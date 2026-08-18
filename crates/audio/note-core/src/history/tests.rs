//! History 模块单元测试
//!
//! 覆盖：
//! - 基础 push / undo / redo / clear 行为
//! - EditorSnapshot 构造
//! - 元数据：group_id 分配 / op_kind / entry_count
//!
//! 拆分说明（避免单文件超 400 行）：
//! - `tests/merge.rs`：合并窗口（push_mergeable）与 set_config 测试
//! - `tests/logical.rs`：逻辑撤销/重做（undo_logical / redo_logical）测试
//! - `tests/move_op.rs`：MoveOp inverse / push_move_op / 混合 undo-redo 测试

use super::*;
use std::sync::Arc;

use lumino_midi_model::{ChunkedList, NoteEvent};

mod logical;
mod merge;
mod move_op;

fn make_snapshot(notes_len: usize) -> EditorSnapshot {
    let mut notes: Vec<NoteEvent> = Vec::with_capacity(notes_len);
    for i in 0..notes_len {
        notes.push(NoteEvent::new(i as u32, i as u32 + 1, 60, 100, 0));
    }
    EditorSnapshot::new(Arc::new(ChunkedList::from_sorted(notes)), 0, Vec::new())
}

fn assert_snapshot(entry: &HistoryEntry) -> &EditorSnapshot {
    match entry {
        HistoryEntry::Snapshot(bx) => bx,
        HistoryEntry::Operation(_) => panic!("期望 Snapshot，得到 Operation"),
        HistoryEntry::Create(_) => panic!("期望 Snapshot，得到 Create"),
    }
}

fn assert_operation(entry: &HistoryEntry) -> &OperationEntry {
    match entry {
        HistoryEntry::Snapshot(_) => panic!("期望 Operation，得到 Snapshot"),
        HistoryEntry::Operation(o) => o,
        HistoryEntry::Create(_) => panic!("期望 Operation，得到 Create"),
    }
}

fn op_kind_of(entry: &HistoryEntry) -> OpKind {
    match entry {
        HistoryEntry::Snapshot(bx) => bx.op_kind,
        HistoryEntry::Operation(o) => o.op_kind,
        HistoryEntry::Create(_) => OpKind::NoteCreate,
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
    assert_eq!(
        assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目"))
            .notes
            .len(),
        5
    );
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

    let snap1 = history.undo(make_snapshot(3)).expect("撤销应成功");
    assert_eq!(assert_snapshot(&snap1).notes.len(), 2);

    let snap2 = history.redo(make_snapshot(2)).expect("重做应成功");
    assert_eq!(assert_snapshot(&snap2).notes.len(), 3);

    // 再次 undo 应能回到 snap2
    let snap3 = history.undo(make_snapshot(3)).expect("撤销应成功");
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

    let back = assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目"));
    assert_eq!(back.op_kind, OpKind::NoteDelete);
    assert_eq!(back.entry_count, 1);
    assert!(back.parent_group_id.is_none(), "首次 push 无 parent");
    let gid = back.group_id.expect("push_with_op_kind 必须分配 group_id");

    // 第二次 push 应分配不同的 group_id
    history.push_with_op_kind(make_snapshot(2), OpKind::NoteDelete);
    let back2 = assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目"));
    assert_ne!(
        back2.group_id.expect("快照应属于某分组"),
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
