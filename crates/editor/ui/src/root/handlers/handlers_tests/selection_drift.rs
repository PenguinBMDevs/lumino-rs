//! 远端结构编辑后的主选择漂移防护回归测试
//!
//! 背景（用户报告）：A 端选中一段音符后，B 端在同轨头部插入音符——主选择
//! 以当前轨索引存储，远端编辑位移索引后 A 端选中集指向错误音符，后续
//! Delete/移调会命中错误音符（数据损坏）。
//!
//! 修复：`apply_remote_note_operation_now` 以 `SelectionIdentity` guard 包裹
//! （编辑前按 id 捕获、编辑后重映射），覆盖 Add/Delete/Move/Update 四类操作。
//! P3 的临界区延迟补放只保护手势期间；本测试覆盖**空闲时立即应用**的路径。

use super::{attach_test_document, create_root};
use crate::root::Root;
use lumino_collaboration::types::{NoteAction, NoteBatchOperation, SyncNote};
use lumino_note_core::note::Note;

/// 构造远端音符批量操作（单音符）
fn make_op(
    action: NoteAction,
    track: usize,
    id: u64,
    tick: f32,
    key: u16,
    tick_offset: Option<f32>,
) -> NoteBatchOperation {
    NoteBatchOperation {
        action,
        notes: vec![SyncNote {
            id,
            tick,
            key,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: track,
        }],
        source_track: Some(track),
        target_track: Some(track),
        tick_offset,
        key_offset: Some(0),
        timestamp: 0,
    }
}

/// 当前轨（1）选中音符的 id 列表（升序）
fn selected_ids(root: &Root) -> Vec<u64> {
    let notes = root.editor.editor_state.data.track_notes(1);
    let mut ids: Vec<u64> = root
        .editor
        .get_selected_indices()
        .into_iter()
        .filter_map(|i| notes.get(i))
        .map(|ev| ev.id)
        .collect();
    ids.sort_unstable();
    ids
}

/// 当前选中索引（升序）
fn selected_indices(root: &Root) -> Vec<usize> {
    let mut v = root.editor.get_selected_indices();
    v.sort_unstable();
    v
}

/// 在当前轨（1）按升序 tick 写入音符，返回其 id
fn seed(root: &mut Root, tick: f32, key: u16) -> u64 {
    root.editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(tick, key, 480.0, 100, 0));
    let notes = root.editor.editor_state.data.track_notes(1);
    let last = notes.len() - 1;
    notes.get(last).expect("刚插入的音符应存在").id
}

/// 种子 3 个音符（tick 480/960/1440）并返回其 id
fn seed_three(root: &mut Root) -> (u64, u64, u64) {
    (
        seed(root, 480.0, 62),
        seed(root, 960.0, 64),
        seed(root, 1440.0, 66),
    )
}

/// 用户报告场景：远端同轨头部插入 → 本地选中不得漂移到错误音符
#[test]
fn test_remote_add_preserves_selection_identity() {
    let mut root = create_root();
    attach_test_document(&mut root);
    let (_id1, id2, id3) = seed_three(&mut root);
    // 选中后两个音符（索引 1、2）
    root.editor.selection_insert(1);
    root.editor.selection_insert(2);

    // 远端在轨 1 头部插入新音符（tick 0）→ 既有索引整体右移 1
    root.apply_remote_note_operation(&make_op(NoteAction::Add, 1, 900, 0.0, 50, None));

    assert_eq!(
        selected_ids(&root),
        vec![id2, id3],
        "远端插入后选中不得漂移到其它音符"
    );
    assert_eq!(selected_indices(&root), vec![2, 3], "选中索引应随插入右移");
}

#[test]
fn test_remote_delete_preserves_selection_identity() {
    let mut root = create_root();
    attach_test_document(&mut root);
    let (id1, id2, id3) = seed_three(&mut root);
    // 选中首尾两个（索引 0、2）
    root.editor.selection_insert(0);
    root.editor.selection_insert(2);

    // 远端删除中间音符 → 尾部索引左移
    root.apply_remote_note_operation(&make_op(NoteAction::Delete, 1, id2, 960.0, 64, None));

    assert_eq!(
        selected_ids(&root),
        vec![id1, id3],
        "远端删除后选中不得漂移（id3 应随左移保持选中）"
    );
    assert_eq!(selected_indices(&root), vec![0, 1], "选中索引应随删除左移");
}

#[test]
fn test_remote_move_preserves_selection_identity() {
    let mut root = create_root();
    attach_test_document(&mut root);
    let (id1, id2, id3) = seed_three(&mut root);
    // 选中首个音符（索引 0）
    root.editor.selection_insert(0);

    // 远端把首个音符右移 5000 tick（落末尾：id2/id3 左移，id1 到索引 2）
    root.apply_remote_note_operation(&make_op(NoteAction::Move, 1, id1, 480.0, 62, Some(5000.0)));

    assert_eq!(selected_ids(&root), vec![id1], "远端移动后选中应跟随该音符");
    assert_eq!(selected_indices(&root), vec![2], "id1 应落在新位置索引 2");
    // 排序不变式：id2、id3 在 id1 之前
    let notes = root.editor.editor_state.data.track_notes(1);
    assert_eq!(notes.get(0).expect("音符").id, id2);
    assert_eq!(notes.get(1).expect("音符").id, id3);
}
