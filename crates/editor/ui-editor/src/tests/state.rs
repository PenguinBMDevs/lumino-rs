//! 状态相关测试（内存、远程、音频、撤销、音符变速等）
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。

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

// ===== 音符变速功能测试 =====

/// 测试空音符列表变速
#[test]
fn test_speed_change_empty_notes() {
    let mut editor = Editor::new();
    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 0);
}

/// 测试全部音符变速
#[test]
fn test_speed_change_all_notes() {
    let mut editor = Editor::new();
    // 音符A: tick=0, length=480
    // 音符B: tick=600, length=240
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(600.0, 62, 240.0)],
    );

    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // 以最早 tick(0) 为锚点缩放
    // A: tick'=0+(0-0)*0.5=0, length'=240
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 240.0).abs() < f32::EPSILON);
    // B: tick'=0+(600-0)*0.5=300, length'=120
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 300.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").length - 120.0).abs() < f32::EPSILON);
}

/// 测试仅选中的音符变速
#[test]
fn test_speed_change_selected_notes_only() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 480.0),
            Note::new(600.0, 62, 240.0),
            Note::new(1200.0, 64, 120.0),
        ],
    );

    // 只选中第 1 和第 3 个音符
    editor.editor_state.interaction.selected_notes.insert(0);
    editor.editor_state.interaction.selected_notes.insert(2);

    let modified = editor.apply_speed_change(2.0);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // A 选中: tick'=0, length'=960
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 960.0).abs() < f32::EPSILON);
    // B 未选中: 不变
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 600.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").length - 240.0).abs() < f32::EPSILON);
    // C 选中: tick'=0+(1200-0)*2=2400, length'=240
    assert!((data.get_note_view(2).expect("第 3 个音符视图应存在").tick - 2400.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(2).expect("第 3 个音符视图应存在").length - 240.0).abs() < f32::EPSILON);
}

/// 测试变速时最小长度限制
#[test]
fn test_speed_change_clamp_to_min_length() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 10.0)]);

    let modified = editor.apply_speed_change(0.01);
    assert_eq!(modified, 1);

    let data = &editor.editor_state.data;
    // tick 缩放: 100+(100-100)*0.01=100
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 100.0).abs() < f32::EPSILON);
    // 最小长度为 1 tick
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 1.0).abs() < f32::EPSILON);
}

/// 测试变速因子为 1 时无变化
#[test]
fn test_speed_change_no_op_when_factor_is_one() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    let modified = editor.apply_speed_change(1.0);
    assert_eq!(modified, 0);

    let data = &editor.editor_state.data;
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 480.0).abs() < f32::EPSILON);
}

/// 测试变速后撤销/重做
#[test]
fn test_speed_change_undo_redo() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(600.0, 62, 240.0)],
    );

    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 240.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 300.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").length - 120.0).abs() < f32::EPSILON);

    // 撤销
    let undo_result = editor.undo();
    assert!(undo_result);

    let data = &editor.editor_state.data;
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 480.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 600.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").length - 240.0).abs() < f32::EPSILON);
}

/// 关键测试：尾部贴合的音符变速后仍然贴合
#[test]
fn test_speed_change_preserves_adjacent_notes() {
    let mut editor = Editor::new();
    // A: tick=100, length=200 → 结束于 300
    // B: tick=300, length=150 → 开始于 300
    // A 和 B 尾部贴合
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(100.0, 60, 200.0), Note::new(300.0, 62, 150.0)],
    );

    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // A: tick'=100+(100-100)*0.5=100, length'=100 → 结束于 200
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 100.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 100.0).abs() < f32::EPSILON);
    // B: tick'=100+(300-100)*0.5=200, length'=75 → 开始于 200
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 200.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").length - 75.0).abs() < f32::EPSILON);

    // 验证贴合: A.end == B.start
    let a_end = data.get_note_view(0).expect("第 1 个音符视图应存在").tick + data.get_note_view(0).expect("第 1 个音符视图应存在").length;
    let b_start = data.get_note_view(1).expect("第 2 个音符视图应存在").tick;
    assert!(
        (a_end - b_start).abs() < f32::EPSILON,
        "尾部贴合关系被破坏: A.end={}, B.start={}",
        a_end,
        b_start
    );
}

/// 验证有间隙的音符保持相对间隙比例
#[test]
fn test_speed_change_preserves_gap_ratio() {
    let mut editor = Editor::new();
    // A: tick=0, length=100 → 结束于 100
    // B: tick=200, length=100 → 开始于 200
    // 间隙 = 100 ticks
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(200.0, 62, 100.0)],
    );

    let modified = editor.apply_speed_change(2.0);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // A: tick'=0, length'=200 → 结束于 200
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(0).expect("第 1 个音符视图应存在").length - 200.0).abs() < f32::EPSILON);
    // B: tick'=0+(200-0)*2=400, length'=200
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 400.0).abs() < f32::EPSILON);
    assert!((data.get_note_view(1).expect("第 2 个音符视图应存在").length - 200.0).abs() < f32::EPSILON);

    // 验证间隙比例: 原始间隙=100, 缩放后间隙=200
    let original_gap = 200.0 - (0.0 + 100.0); // B.start - A.end
    let new_gap = data.get_note_view(1).expect("第 2 个音符视图应存在").tick
        - (data.get_note_view(0).expect("第 1 个音符视图应存在").tick + data.get_note_view(0).expect("第 1 个音符视图应存在").length);
    assert!(
        (new_gap - original_gap * 2.0).abs() < f32::EPSILON,
        "间隙比例被破坏: 原始={}, 新={}",
        original_gap,
        new_gap
    );
}
