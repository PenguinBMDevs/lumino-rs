//! 派生缓存必须跟随**音符内容世代**失效（审计 §五 同类项）
//!
//! `SpatialIndexState` 的两个派生缓存——`note_index`（当前轨空间索引）与
//! `max_note_len`（框选 lookback 上界）——此前靠 UI 层布尔标记
//! `note_index_dirty` / `max_note_len = 0` 失效，而这两个标记只由
//! `Editor::mark_notes_changed`（UI 层）和切轨路径设置。
//!
//! **数据层**的写入路径只 bump `EditorData::track_notes_gen`，不碰 UI 层标记：
//! 协作远端编辑、素材放置、以及点名过的「找回删除音轨」
//! （`root/editor_ops/track/deletion.rs:169-177` 走 `replace_track_notes`）。
//! 于是：
//!
//! - `note_index` 陈旧 → 框选（`update_query_marquee`）与可见音符收集会命中
//!   **已删除/已移走**的音符（索引下标甚至指向不存在的音符）；
//! - `max_note_len` 陈旧偏小 → 框选窗口 lookback 不足 → 跨入窗口的长音符漏选。
//!
//! 修法：两个缓存都按 `(track_notes_gen, current_track)` 这个权威键失效，
//! 与播放键色索引（§十六）同一套机制。

use std::collections::HashSet;

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_midi_loader::NoteEvent;

/// 手工安装空间索引（小数据量下 `ensure_spatial_index` 不建树，阈值 20 万）
fn force_spatial_index(editor: &Editor) {
    let notes = editor.editor_state.data.current_track_notes();
    let refs: Vec<lumino_note_core::NoteRef> = notes
        .iter()
        .enumerate()
        .map(|(i, n)| lumino_note_core::NoteRef {
            tick: n.start_tick as f32,
            key: n.key as u16,
            length: (n.end_tick - n.start_tick) as f32,
            index: i,
        })
        .collect();
    *editor.spatial.note_index.borrow_mut() =
        Some(lumino_note_core::NoteSpatialIndex::from_note_refs(&refs));
    editor
        .spatial
        .note_index_key
        .set(Some(editor.spatial_cache_key()));
}

/// 模拟数据层写入：整轨替换 + bump 世代（**不**调用 UI 层 `mark_notes_changed`）
///
/// 与 `root/editor_ops/track/deletion.rs:169-177`「找回删除音轨」同形——
/// 那正是审计点名的漏同步路径。
fn data_layer_replace_track(editor: &mut Editor, notes: Vec<NoteEvent>) {
    let data = &mut editor.editor_state.data;
    data.replace_track_notes(0, notes);
    data.mark_track_notes_changed_for(Some(HashSet::from([0])));
}

/// 以显式边界驱动一次框选命中计算（绕过量化写入路径）
fn select_rect(
    editor: &mut Editor,
    start_tick: f32,
    current_tick: f32,
    start_key: u16,
    current_key: u16,
) {
    editor.cached_selection_bounds.set(None);
    editor.selected_bounds.set(None);
    editor.editor_state.interaction.selected_notes.clear();
    editor.editor_state.interaction.edit_state = EditState::Selecting {
        start_tick,
        start_key,
        current_tick,
        current_key,
        start_y: 0.0,
        current_y: 0.0,
    };
    editor.update_selection();
}

/// 数据层改动音符后，陈旧空间索引必须被清除/重建（不得继续复用）
#[test]
fn test_spatial_index_follows_notes_generation() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(480.0, 62, 480.0)],
    );
    force_spatial_index(&editor);
    assert!(
        editor.spatial.note_index.borrow().is_some(),
        "前置：索引已装入"
    );

    // 数据层整轨替换（音符数变 1）——不通知 UI 层
    data_layer_replace_track(&mut editor, vec![NoteEvent::new(0, 240, 64, 100, 0)]);

    editor.ensure_spatial_index();
    assert!(
        editor.spatial.note_index.borrow().is_none(),
        "音符世代已变：陈旧索引必须失效（小型文档走窗口扫描，不得复用旧树）"
    );
}

/// 框选不得选中「已被数据层删除」的音符（陈旧索引会返回不存在的下标）
#[test]
fn test_marquee_does_not_select_removed_notes() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0),
            Note::new(100.0, 60, 100.0),
            Note::new(200.0, 60, 100.0),
        ],
    );
    force_spatial_index(&editor);

    // 数据层清空当前轨（三颗音符全部删除）
    data_layer_replace_track(&mut editor, Vec::new());

    // 框选覆盖原三颗音符的位置：陈旧索引会返回下标 0/1/2（音符已不存在）
    select_rect(&mut editor, 0.0, 300.0, 60, 60);
    assert_eq!(
        editor.selected_notes_count(),
        0,
        "音符已被删除，框选不得命中陈旧索引里的下标"
    );
}

/// `max_note_len` 必须随音符世代重算（陈旧偏小会让框选 lookback 漏长音符）
#[test]
fn test_max_note_len_follows_notes_generation() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);

    // 先让缓存变热
    assert_eq!(
        editor.current_track_max_note_len(),
        100,
        "前置：缓存最长音符长度"
    );

    // 数据层换成一条更长的音符（100 → 5000）
    data_layer_replace_track(&mut editor, vec![NoteEvent::new(0, 5000, 60, 100, 0)]);

    assert_eq!(
        editor.current_track_max_note_len(),
        5000,
        "最长音符长度缓存必须随世代重算：陈旧偏小会导致框选 lookback 不足而漏选"
    );
}

/// 显式 `mark_notes_changed` 必须仍能失效缓存（**回归护栏**）
///
/// 拖动热路径的增量接口（`restore_current_track_sorted_incremental` /
/// `record_update_ranges`，见 `interaction/released.rs` 拉伸提交）刻意**不** bump
/// `track_notes_gen`（每帧全量同步代价过高），它们靠 `Editor::mark_notes_changed`
/// 显式失效。若后人以为「世代键已足够」而删掉这条通路，拉伸音符后空间索引会陈旧：
/// 本测试即该回归的护栏。
#[test]
fn test_explicit_mark_still_invalidates_spatial_index() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    force_spatial_index(&editor);
    assert_eq!(editor.current_track_max_note_len(), 480, "前置：缓存已热");

    // 只走 UI 层显式失效，不 bump 世代
    editor.mark_notes_changed();

    editor.ensure_spatial_index();
    assert!(
        editor.spatial.note_index.borrow().is_none(),
        "显式 mark_notes_changed 后索引必须失效（增量热路径依赖这条通路）"
    );
    assert_eq!(
        editor.current_track_max_note_len(),
        480,
        "显式失效后应重算（值不变但必须走重算路径，而非返回陈旧缓存）"
    );
}
