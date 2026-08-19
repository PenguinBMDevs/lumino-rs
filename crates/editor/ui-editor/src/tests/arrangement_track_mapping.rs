//! 工程走带视觉位置 → 文档音轨索引 映射测试
//!
//! 音轨拖拽排序后，侧边栏顺序（视觉位置）与 document 音轨索引不再一致。
//! 走带操作（添加/擦除/切割）必须经 `track_visual_order` 映射，
//! 否则编辑落在错误的音轨上。本模块验证映射正确性（排序场景）。

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers::{doc_with_notes, seed_notes};

/// 构造 3 轨 document（音符在 doc 轨 0 和轨 2），并设置排序后的视觉映射：
/// 视觉 0 → doc 2，视觉 1 → doc 0，视觉 2 → doc 1（模拟用户把轨 2 拖到顶部）
fn editor_with_sorted_visual_order() -> Editor {
    let mut editor = Editor::default();
    let notes2 = vec![Note::from_raw(0.0, 64, 10.0, 100, 0)];
    editor.editor_state.data.document = Some(doc_with_notes(3, 2, &notes2));
    // 经数据层 API 向 doc 轨 0 写入音符（避免覆盖整个 document）
    editor
        .editor_state
        .data
        .insert_note(0, Note::from_raw(0.0, 60, 10.0, 100, 0));
    // 排序后的视觉映射：视觉 0 = doc 轨 2，视觉 1 = doc 轨 0，视觉 2 = doc 轨 1
    editor.editor_state.data.track_visual_order = vec![2, 0, 1];
    editor
}

fn doc_track_note_count(editor: &Editor, track: usize) -> usize {
    editor.editor_state.data.track_notes(track).iter().count()
}

#[test]
fn test_document_track_at_maps_sorted_visual_order() {
    let editor = editor_with_sorted_visual_order();
    assert_eq!(editor.editor_state.data.document_track_at(0), 2);
    assert_eq!(editor.editor_state.data.document_track_at(1), 0);
    assert_eq!(editor.editor_state.data.document_track_at(2), 1);
}

#[test]
fn test_add_note_lands_on_mapped_document_track() {
    let mut editor = editor_with_sorted_visual_order();
    // 视觉轨 0（对应 doc 轨 2）添加音符
    let added = editor.arrange_add_note(3, 0, 100.0, 10.0, 70, 100);
    assert!(added);
    assert_eq!(doc_track_note_count(&editor, 2), 2, "doc 轨 2 应新增音符");
    assert_eq!(doc_track_note_count(&editor, 0), 1, "doc 轨 0 不应受影响");
    assert_eq!(doc_track_note_count(&editor, 1), 0);
}

#[test]
fn test_erase_uses_mapped_document_track_range() {
    let mut editor = editor_with_sorted_visual_order();
    // 擦除视觉区间 [0, 0]（仅 doc 轨 2）
    let deleted = editor.arrange_erase(0.0, 20.0, 0, 0);
    assert_eq!(deleted, 1);
    assert_eq!(doc_track_note_count(&editor, 2), 0, "doc 轨 2 音符应被擦除");
    assert_eq!(doc_track_note_count(&editor, 0), 1, "doc 轨 0 不应受影响");
}

#[test]
fn test_erase_mapped_range_covers_multiple_tracks() {
    let mut editor = editor_with_sorted_visual_order();
    // 擦除视觉区间 [0, 1]（doc 轨 2 和 doc 轨 0）
    let deleted = editor.arrange_erase(0.0, 20.0, 0, 1);
    assert_eq!(deleted, 2);
    assert_eq!(doc_track_note_count(&editor, 2), 0);
    assert_eq!(doc_track_note_count(&editor, 0), 0);
}

#[test]
fn test_razor_splits_mapped_document_track() {
    let mut editor = editor_with_sorted_visual_order();
    // 切割视觉轨 0（对应 doc 轨 2）上 tick=5 处的音符
    let split = editor.arrange_razor(5.0, 0);
    assert_eq!(split, 1);
    assert_eq!(
        doc_track_note_count(&editor, 2),
        2,
        "doc 轨 2 音符应被一分为二"
    );
    assert_eq!(doc_track_note_count(&editor, 0), 1, "doc 轨 0 不应受影响");
}

#[test]
fn test_mapping_falls_back_to_identity_when_unset() {
    // 未设置视觉映射（恒等）时，视觉位置即文档索引（兼容旧行为）
    let mut editor = Editor::default();
    seed_notes(&mut editor, 2, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    assert_eq!(editor.editor_state.data.track_visual_order.len(), 0);
    assert_eq!(editor.editor_state.data.document_track_at(0), 0);
    assert_eq!(editor.editor_state.data.document_track_at(1), 1);
    // 恒等映射下 add/erase 行为不变
    let added = editor.arrange_add_note(2, 1, 50.0, 10.0, 72, 100);
    assert!(added);
    assert_eq!(doc_track_note_count(&editor, 1), 1);
}
