//! 删除路径协作广播门控测试：`collab_sync_enabled` 关闭时不得逐音符 emit。
//!
//! 背景（2026-09 删除性能修复）：`Editor::delete_selected_notes` 旧实现无条件
//! 构建 O(轨道长度) 的待删捕获缓冲并逐音符 emit `LocalNoteDeleted`。未连接
//! 协作会话（生产默认，`collab_sync_enabled=false`）时消费端短路丢弃，这些
//! 事件纯属浪费（200W 选中 ≈ 100MB 捕获 + 234MB 事件队列）。
//! 现与粘贴/拖动/提交路径一致，仅在协作同步开启时广播。
//!
//! 事件缓冲区是全局单例，本模块用唯一魔数 tick 从并行测试污染中隔离。

use std::sync::Mutex;

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_message::events::window::sync::Event as SyncEvent;
use lumino_message::events::{self, Event};

// 全局事件缓冲区单例：串行化本模块的事件断言（中毒时恢复，避免级联失败）
static EVENT_TEST_MUTEX: Mutex<()> = Mutex::new(());

/// 唯一魔数：本仓库其它测试不会使用的 tick，用于精确隔离并行事件污染。
const SIG_TICKS: [f32; 3] = [7777.0, 7788.0, 7799.0];

fn lock_event_guard() -> std::sync::MutexGuard<'static, ()> {
    EVENT_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
}

/// 取出缓冲区中 tick 命中本测试魔数的 `LocalNoteDeleted` 事件数。
fn count_signature_deletes() -> usize {
    events::take_events()
        .into_iter()
        .filter(|e| {
            matches!(
                e,
                Event::Window(lumino_message::events::window::Event::Sync(
                    SyncEvent::LocalNoteDeleted { tick, .. }
                )) if SIG_TICKS.contains(tick)
            )
        })
        .count()
}

fn seed_magic_notes(editor: &mut Editor) {
    let notes: Vec<Note> = SIG_TICKS
        .iter()
        .enumerate()
        .map(|(i, t)| Note::new(*t, 60 + i as u16, 10.0))
        .collect();
    test_helpers::seed_notes(editor, 1, 0, &notes);
}

#[test]
fn test_delete_selected_without_collab_emits_nothing() {
    let _guard = lock_event_guard();
    let _ = events::take_events();

    let mut editor = Editor::new();
    seed_magic_notes(&mut editor);
    // 生产默认：未连接协作会话 → collab_sync_enabled=false
    assert!(!editor.editor_state.data.collab_sync_enabled());
    editor.select_all_notes();
    assert_eq!(editor.selected_notes_count(), SIG_TICKS.len());

    editor.delete_selected_notes();

    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "删除本身必须生效（门控只影响广播，不影响数据）"
    );
    assert_eq!(
        count_signature_deletes(),
        0,
        "协作关闭时不得逐音符广播 LocalNoteDeleted"
    );
}

#[test]
fn test_delete_selected_with_collab_emits_each_note() {
    let _guard = lock_event_guard();
    let _ = events::take_events();

    let mut editor = Editor::new();
    seed_magic_notes(&mut editor);
    editor.editor_state.data.set_collab_sync_enabled(true);
    editor.select_all_notes();

    editor.delete_selected_notes();

    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    assert_eq!(
        count_signature_deletes(),
        SIG_TICKS.len(),
        "协作开启时每个被删音符应广播一次 LocalNoteDeleted"
    );
}

#[test]
fn test_delete_selected_undo_restores_at_editor_level() {
    let _guard = lock_event_guard();
    let _ = events::take_events();

    let mut editor = Editor::new();
    seed_magic_notes(&mut editor);
    editor.select_all_notes();
    editor.delete_selected_notes();
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);

    assert!(editor.undo(), "删除后应可撤销");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        SIG_TICKS.len(),
        "撤销必须恢复全部被删音符"
    );
}
