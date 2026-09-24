//! 远端音符操作与本地编辑临界区串行化回归测试
//!
//! 背景：本地拖动/待提交/异步提交期间，`DragState.selected`（BitVec）与
//! `note_index` 引用当前轨**索引**；若远端结构编辑（增/删/移）此时落到同一轨，
//! 索引漂移会让拖动/提交引用错误音符（数据损坏）。修复：同轨远端结构编辑
//! 延迟到本地临界区结束后按到达顺序补放（`Root::drain_deferred_remote_ops`）。

use super::{attach_test_document, create_root};
use crate::root::Root;
use lumino_collaboration::types::{NoteAction, NoteBatchOperation, SyncNote};
use lumino_editor_state::{DragState, EditState};
use lumino_note_core::note::Note;
use lumino_ui_core::message::EditorAction;

/// 构造远端「添加音符」操作
fn make_add_op(track: usize, id: u64, tick: f32, key: u16) -> NoteBatchOperation {
    NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![SyncNote {
            id,
            tick,
            key,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: track,
        }],
        source_track: None,
        target_track: None,
        tick_offset: None,
        key_offset: None,
        timestamp: 0,
    }
}

/// 在 track 1（当前轨）写入一个测试音符
fn seed_track1_note(root: &mut Root, tick: f32, key: u16) {
    root.editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(tick, key, 480.0, 100, 0));
}

/// 模拟进行中的批量拖动（当前轨 1 选中索引 0，delta +5 tick）
fn start_live_drag(root: &mut Root) {
    let count = root.editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices([0], count, 0, 60);
    drag.set_delta(5, 0);
    root.editor.editor_state.interaction.edit_state =
        EditState::DraggingSelection { drag_state: drag };
}

#[test]
fn test_remote_op_applies_immediately_when_idle() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = create_root();
    attach_test_document(&mut root);
    seed_track1_note(&mut root, 0.0, 60);
    let before = root.editor.editor_state.data.track_notes(1).len();

    root.apply_remote_note_operation(&make_add_op(1, 900, 960.0, 72));

    assert_eq!(
        root.editor.editor_state.data.track_notes(1).len(),
        before + 1,
        "空闲时远端操作应立即应用"
    );
    assert!(root.deferred_remote_ops.is_empty());
}

#[test]
fn test_remote_op_deferred_during_live_drag_then_drained() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = create_root();
    attach_test_document(&mut root);
    seed_track1_note(&mut root, 0.0, 60);
    start_live_drag(&mut root);
    let before = root.editor.editor_state.data.track_notes(1).len();

    root.apply_remote_note_operation(&make_add_op(1, 901, 960.0, 72));

    assert_eq!(
        root.editor.editor_state.data.track_notes(1).len(),
        before,
        "拖动进行中远端同轨操作应被延迟"
    );
    assert_eq!(root.deferred_remote_ops.len(), 1);

    // 模拟松手（清除活跃拖动）→ 每帧补放
    root.editor.editor_state.interaction.edit_state = EditState::Idle;
    root.drain_deferred_remote_ops();

    assert_eq!(
        root.editor.editor_state.data.track_notes(1).len(),
        before + 1,
        "临界区结束后应补放延迟操作"
    );
    assert!(root.deferred_remote_ops.is_empty());
}

#[test]
fn test_remote_op_other_track_applies_immediately_during_drag() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = create_root();
    attach_test_document(&mut root);
    seed_track1_note(&mut root, 0.0, 60);
    start_live_drag(&mut root); // 拖动发生在当前轨 1
    let before0 = root.editor.editor_state.data.track_notes(0).len();

    root.apply_remote_note_operation(&make_add_op(0, 902, 0.0, 62));

    assert_eq!(
        root.editor.editor_state.data.track_notes(0).len(),
        before0 + 1,
        "非当前轨的远端操作不影响本地索引，应立即应用"
    );
    assert!(root.deferred_remote_ops.is_empty());
}

#[test]
fn test_remote_op_deferred_then_pending_drag_autocommit_and_drain() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = create_root();
    attach_test_document(&mut root);
    seed_track1_note(&mut root, 0.0, 60);
    start_live_drag(&mut root);

    // 松手 → 进入待提交（pending_drag_state）
    root.editor.handle_action(EditorAction::Released);
    assert!(root.editor.has_uncommitted_drag(), "松手后应存在待提交拖动");

    // 远端同轨操作：临界区（待提交）期间应延迟
    root.apply_remote_note_operation(&make_add_op(1, 903, 960.0, 72));
    assert_eq!(root.deferred_remote_ops.len(), 1);

    // 每帧补放：自动提交待提交拖动（异步）→ 等待完成 → 补放远端操作
    root.drain_deferred_remote_ops();
    assert!(
        root.editor.editor_state.data.has_pending_commit(),
        "补放应自动提交已松手的待提交拖动"
    );
    // 等待异步提交完成
    loop {
        if root.editor.poll_async_commit().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    root.drain_deferred_remote_ops();

    assert!(root.deferred_remote_ops.is_empty(), "远端操作应已补放");
    let track = root.editor.editor_state.data.track_notes(1);
    // 本地拖动已提交生效（tick 0 → 5）
    assert!(
        track.iter().any(|n| n.start_tick == 5 && n.key == 60),
        "本地拖动应已提交生效，实际音符: {:?}",
        track.to_vec()
    );
    // 远端音符已补放
    assert!(
        track.iter().any(|n| n.id == 903),
        "远端音符应已补放写入，实际音符: {:?}",
        track.to_vec()
    );
}
