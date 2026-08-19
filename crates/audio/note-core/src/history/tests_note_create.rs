//! NoteCreate 增量操作日志（CreateOp / CreateEntry）单元测试
//!
//! 拆分原因：`tests.rs` 超 400 行限制，将音符创建增量日志的测试独立成文件。

use super::*;
use std::sync::Arc;

use lumino_midi_model::{ChunkedList, NoteEvent};

fn make_snapshot(notes_len: usize) -> EditorSnapshot {
    let mut notes: Vec<NoteEvent> = Vec::with_capacity(notes_len);
    for i in 0..notes_len {
        notes.push(NoteEvent::new(i as u32, i as u32 + 1, 60, 100, 0));
    }
    EditorSnapshot::new(Arc::new(ChunkedList::from_sorted(notes)), 0, Vec::new())
}

fn assert_create(entry: &HistoryEntry) -> &CreateEntry {
    match entry {
        HistoryEntry::Snapshot(_) => panic!("期望 Create，得到 Snapshot"),
        HistoryEntry::Operation(_) => panic!("期望 Create，得到 Operation"),
        HistoryEntry::Create(c) => c,
    }
}

fn make_create_op(track_id: u32, tick: u32) -> CreateOp {
    CreateOp {
        track_id,
        note: NoteEvent::new(tick, tick + 100, 60, 100, 0),
    }
}

#[test]
fn test_push_note_create_creates_entry() {
    let mut history = History::new();
    let op = make_create_op(1, 0);
    let merged = history.push_note_create(vec![op]);
    assert!(!merged, "首次 push 应新增分组");
    assert_eq!(history.undo_len(), 1);

    let back = history.undo_back().expect("undo 栈顶应存在");
    let entry = assert_create(back);
    assert_eq!(entry.ops.len(), 1);
    assert_eq!(entry.ops[0].track_id, 1);
    assert_eq!(entry.ops[0].note.start_tick, 0);
    assert_eq!(entry.group_id, Some(1));
    assert_eq!(entry.entry_count, 1);
}

#[test]
fn test_push_note_create_within_window_merges() {
    let mut history = History::new();
    history.push_note_create(vec![make_create_op(1, 0)]);
    // 窗口内第二次 push 合并到同一条（entry_count 2，ops 追加）
    let merged = history.push_note_create(vec![make_create_op(1, 100)]);
    assert!(merged, "窗口内应合并");
    assert_eq!(history.undo_len(), 1);

    let entry = assert_create(history.undo_back().expect("应存在可撤销的历史条目"));
    assert_eq!(entry.ops.len(), 2);
    assert_eq!(entry.entry_count, 2);
    assert_eq!(entry.ops[0].note.start_tick, 0);
    assert_eq!(entry.ops[1].note.start_tick, 100, "ops 保持时间正序");
}

#[test]
fn test_push_note_create_split_on_limit() {
    let mut history = History::with_config(100, 300, 2);
    history.push_note_create(vec![make_create_op(1, 0)]);
    history.push_note_create(vec![make_create_op(1, 100)]);
    // 达到上限（entry_count=2），第三次触发分割为新分组
    let merged = history.push_note_create(vec![make_create_op(1, 200)]);
    assert!(!merged, "超限应分割为新分组");
    assert_eq!(history.undo_len(), 2);

    // 栈顶（最新）分组的 parent_group_id 指向旧分组
    let top = assert_create(history.undo_back().expect("应存在可撤销的历史条目"));
    assert_eq!(top.entry_count, 1);
    let parent = top.parent_group_id.expect("分割组应有 parent");
    let first = assert_create(&history.undo_stack[0]);
    assert_eq!(first.group_id, Some(parent));
}

#[test]
fn test_push_note_create_outside_window_new_group() {
    // merge_window=0 → 永不合并，每次 push 都是新分组（语义：0 窗口 = 不合并）
    let mut history = History::with_config(100, 0, 100);
    assert!(!history.push_note_create(vec![make_create_op(1, 0)]));
    assert!(!history.push_note_create(vec![make_create_op(1, 100)]));
    assert_eq!(history.undo_len(), 2);
}

#[test]
fn test_note_create_undo_redo_roundtrip() {
    let mut history = History::new();
    history.push_note_create(vec![
        make_create_op(1, 0),
        make_create_op(1, 100),
        make_create_op(1, 200),
    ]);

    // undo：原样返回 Create entry（编辑侧按 inverse=true 删除音符）
    let entry = history.undo(make_snapshot(0)).expect("undo 应返回 Create");
    let create = assert_create(&entry);
    assert_eq!(create.ops.len(), 3);
    assert_eq!(history.redo_len(), 1, "Create undo 只推入一个原样 Create");
    assert_eq!(history.undo_len(), 0);

    // redo：原样返回（编辑侧按 inverse=false 重新插入）
    let entry = history.redo(make_snapshot(0)).expect("redo 应返回 Create");
    let create = assert_create(&entry);
    assert_eq!(create.ops.len(), 3);
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn test_note_create_logical_undo_across_split_chain() {
    let mut history = History::with_config(100, 100, 3);
    // 4 次创建：分组 1（3 条）+ 分组 2（1 条，parent=分组1）
    for tick in [0u32, 100, 200, 300] {
        history.push_note_create(vec![make_create_op(1, tick)]);
    }
    assert_eq!(history.undo_len(), 2);

    // 逻辑撤销应跨 chain 合并所有 create op，一次性返回全部 4 条
    let entry = history
        .undo_logical(make_snapshot(0))
        .expect("Create 链逻辑撤销应跨组");
    let create = assert_create(&entry);
    assert_eq!(create.ops.len(), 4, "跨组逻辑撤销应合并全部 4 个 op");
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 2, "两条 Create 搬回 redo 栈");

    // 逻辑重做：往返恢复 4 条 op
    let entry = history
        .redo_logical(make_snapshot(0))
        .expect("Create 链逻辑重做应返回");
    let create = assert_create(&entry);
    assert_eq!(create.ops.len(), 4);
    assert_eq!(history.undo_len(), 2);
    assert_eq!(history.redo_len(), 0);
    // 时间正序保持：最早 tick 在前
    assert_eq!(create.ops[0].note.start_tick, 0);
    assert_eq!(create.ops[3].note.start_tick, 300);
}

#[test]
fn test_note_create_mixed_with_snapshot_undo_order() {
    let mut history = History::new();
    history.push(make_snapshot(1)); // 先推一个快照
    history.push_note_create(vec![make_create_op(1, 0)]); // 再推 Create

    // undo 应先返回 Create
    let first = history.undo(make_snapshot(2)).expect("先 undo Create");
    assert!(matches!(first, HistoryEntry::Create(_)));
    // 再 undo 返回 Snapshot
    let second = history.undo(make_snapshot(1)).expect("再 undo Snapshot");
    assert!(matches!(second, HistoryEntry::Snapshot(_)));
}
