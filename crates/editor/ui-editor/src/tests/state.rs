//! 状态相关测试（内存、远程、音频、撤销、音符变速等）
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。
//! 音符变速测试见 `speed_change` 子模块。

mod speed_change;

use crate::CacheInvalidation;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_ui_core::message::AudioAction;
use std::sync::Arc;

// ===== 基础状态测试 =====

/// 测试音符创建
#[test]
fn test_note_creation() {
    let tick = 0.0;
    let key = 60u16;
    let length = 480.0;

    let note = Note::new(tick, key, length);

    assert_eq!(note.tick, tick);
    assert_eq!(note.key, key);
    assert_eq!(note.length, length);
}

/// 测试空编辑器的内存快照
#[test]
fn test_memory_breakdown_empty_editor() {
    let editor = Editor::new();
    let mem = editor.memory_breakdown();

    assert_eq!(mem.track_notes_count, 0);
    assert_eq!(mem.track_notes_entries, 0);
    assert_eq!(mem.document_events_bytes, 0);
}

/// 测试有音符时的内存快照
#[test]
fn test_memory_breakdown_with_notes() {
    let mut editor = Editor::new();
    // 2026-08 单一权威源：音符存入 document（track_notes 缓存已删除）
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 62, 240.0)]);

    let mem = editor.memory_breakdown();
    let note_size = std::mem::size_of::<lumino_midi_loader::NoteEvent>();

    assert_eq!(mem.track_notes_count, 1);
    assert_eq!(mem.track_notes_entries, 1);
    // 2026-08-15：字节统计统一走 document_events_bytes（唯一真实持有）。
    // 按 capacity 统计（含 tempo_changes 等），至少覆盖 1 个音符的 16B
    assert!(mem.document_events_bytes >= note_size);
}

/// 测试远程光标更新与移除
#[test]
fn test_remote_cursor_update_and_remove() {
    let mut editor = Editor::new();

    editor.update_remote_cursor(
        Arc::from("user_1"),
        100.0,
        200.0,
        Arc::from("#ff0000"),
        Arc::from("alice"),
    );
    assert!(editor.remote_cursors.contains_key("user_1"));

    editor.remove_remote_cursor("user_1");
    assert!(!editor.remote_cursors.contains_key("user_1"));
}

/// 测试音频动作推入与取出
#[test]
fn test_take_audio_actions_returns_pushed_actions() {
    let mut editor = Editor::new();
    assert!(editor.take_audio_actions().is_empty());

    editor
        .editor_state
        .interaction
        .push_audio_action(AudioAction::PlayNote {
            key: 60,
            velocity: 100,
        });
    editor
        .editor_state
        .interaction
        .push_audio_action(AudioAction::StopNote { key: 60 });

    let actions = editor.take_audio_actions();
    assert_eq!(actions.len(), 2);
    assert!(editor.take_audio_actions().is_empty());
}

/// 测试 notes_changed 标志位
#[test]
fn test_notes_changed_flag() {
    let mut editor = Editor::new();
    assert!(!editor.notes_changed());

    editor.mark_notes_changed();
    assert!(editor.notes_changed());

    editor.clear_notes_changed();
    assert!(!editor.notes_changed());
}

/// 测试缓存失效
#[test]
fn test_invalidate_caches_clears_specified_cache() {
    let mut editor = Editor::new();
    // canvas::Cache 没有 is_empty，只能验证方法不 panic
    editor.invalidate_caches(CacheInvalidation::GRID);
    editor.invalidate_caches(CacheInvalidation::KEYBOARD);
    editor.invalidate_caches(CacheInvalidation::RULER);
    editor.invalidate_caches(CacheInvalidation::ALL);
}

/// 测试内部状态重置
#[test]
fn test_reset_internal_state() {
    let mut editor = Editor::new();
    editor.playback_position = 123.0;
    editor.notes_changed = true;

    editor.reset_internal_state();

    assert!(!editor.notes_changed());
    assert!((editor.playback_position - 0.0).abs() < f32::EPSILON);
}

/// 测试初始状态下撤销/重做不可用
#[test]
fn test_can_undo_redo_initial_state() {
    let editor = Editor::new();
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
}

/// 测试设置总 ticks 会更新 max_scroll
#[test]
fn test_set_total_ticks_updates_max_scroll() {
    let mut editor = Editor::new();
    editor.set_total_ticks(2000);
    assert_eq!(editor.editor_state.view.total_ticks, 2000);
    assert!(editor.editor_state.max_scroll.0 > 0.0);
}

/// 测试设置 PPQ
#[test]
fn test_set_ppq() {
    let mut editor = Editor::new();
    editor.set_ppq(960);
    assert_eq!(editor.editor_state.view.ppq, 960);
}
