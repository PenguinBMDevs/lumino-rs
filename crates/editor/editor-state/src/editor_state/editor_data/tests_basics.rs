//! 基础构造/重置/访问器测试

use crate::editor_state::constants::DEFAULT_BPM;
use lumino_note_core::note::Note;

use super::EditorData;

#[test]
fn test_editor_data_default() {
    let data = EditorData::default();
    assert_eq!(data.current_track_note_count(), 0);
    assert_eq!(data.current_track, 0);
    assert_eq!(data.track_notes_gen, 0);
    assert!(data.document.is_none());
}

#[test]
fn test_editor_data_new() {
    let data = EditorData::new();
    assert_eq!(data.tempo_points.len(), 1);
    assert_eq!(data.tempo_points[0].bpm, DEFAULT_BPM);
}

#[test]
fn test_reset_clears_data() {
    let mut data = EditorData::with_f32_notes(1, &[Note::new(0.0, 60, 1.0)]);
    data.reset();
    assert_eq!(data.current_track_note_count(), 0);
    assert!(data.document.is_none(), "reset 后 document 应释放");
    assert_eq!(data.track_notes_gen, 1);
}

#[test]
fn test_mark_track_notes_changed() {
    let mut data = EditorData::new();
    data.mark_track_notes_changed();
    assert_eq!(data.track_notes_gen, 1);
    // 未知来源 → onion_dirty_tracks = None（洋葱皮保守全量重建）
    assert!(data.onion_dirty_tracks.is_none());
}

#[test]
fn test_mark_track_notes_changed_for_records_tracks() {
    let mut data = EditorData::new();
    data.current_track = 3;
    data.mark_current_track_changed();
    assert_eq!(data.track_notes_gen, 1);
    assert_eq!(
        data.onion_dirty_tracks,
        Some(std::collections::HashSet::from([3]))
    );
}

#[test]
fn test_mark_track_notes_changed_for_multi_track() {
    let mut data = EditorData::new();
    data.mark_track_notes_changed_for(Some(std::collections::HashSet::from([1, 2])));
    assert_eq!(data.track_notes_gen, 1);
    assert_eq!(
        data.onion_dirty_tracks,
        Some(std::collections::HashSet::from([1, 2]))
    );
}

#[test]
fn test_mark_track_notes_changed_for_none_after_some() {
    // None 覆盖 Some：未知变化必须压制之前的明确豁免信息
    let mut data = EditorData::new();
    data.mark_current_track_changed();
    data.mark_track_notes_changed();
    assert!(data.onion_dirty_tracks.is_none());
}

/// `insert_note` / `remove_note` / `update_note` 必须 bump `track_notes_gen`
///
/// 2026-09 缺陷：三者都只置 `modified` 与增量事件，**从不** bump 版本号，
/// 违反 `mark_track_notes_changed` 的契约。后果是所有按 gen 失效的派生缓存
/// （走带滚动条范围 `cached_max_tick_end`、`OnionSkinState`、走带选区命中音符）
/// 在单音增删改后读到脏数据。
#[test]
fn test_single_note_write_paths_bump_track_notes_gen() {
    let mut data = EditorData::with_f32_notes(1, &[Note::new(0.0, 60, 1.0)]);
    data.current_track = 0;
    let base = data.track_notes_gen;

    assert!(data.insert_note(0, Note::new(2.0, 62, 1.0)));
    assert_eq!(
        data.track_notes_gen,
        base + 1,
        "insert_note 必须 bump gen（否则选区/滚动条缓存读脏）"
    );

    let after_insert = data.track_notes_gen;
    assert!(data.update_note(0, 0, Note::new(5.0, 60, 1.0)));
    assert_eq!(
        data.track_notes_gen,
        after_insert + 1,
        "update_note 必须 bump gen（tick 变了但音符数没变，指望音符数判失效会漏）"
    );

    let after_update = data.track_notes_gen;
    assert!(data.remove_note(0, 0).is_some());
    assert_eq!(
        data.track_notes_gen,
        after_update + 1,
        "remove_note 必须 bump gen"
    );
}

/// 跨轨增删改必须**精确**标记受影响轨，不能按 `{current_track}` 标记
///
/// 否则洋葱皮会对真实变化轨错误豁免重建 → 该轨新增音符漏渲染。
#[test]
fn test_cross_track_write_marks_actual_track_not_current() {
    let mut data = EditorData::with_f32_notes(2, &[Note::new(0.0, 60, 1.0)]);
    data.current_track = 0;
    data.mark_track_notes_changed(); // 清掉构造期的标记

    assert!(
        data.insert_note(1, Note::new(0.0, 64, 1.0)),
        "轨 1 应存在且可插入"
    );
    assert_eq!(
        data.onion_dirty_tracks,
        Some(std::collections::HashSet::from([1])),
        "跨轨插入必须标记真实受影响轨 1，而非当前轨 0"
    );
}

#[test]
fn test_select_all_notes() {
    let data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0), Note::new(1.0, 62, 1.0)]);
    let selected = data.select_all_notes();
    assert_eq!(selected.len(), 2);
}

#[test]
fn test_get_notes_in_selection_box() {
    let data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0), Note::new(5.0, 62, 1.0)]);

    let indices = data.get_notes_in_selection_box(-1.0, 59, 3.0, 61);
    assert_eq!(indices.len(), 1);
    assert_eq!(indices[0], 0);
}

#[test]
fn test_compute_selection() {
    let data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0)]);
    let selected = data.compute_selection(-1.0, 59, 3.0, 61);
    assert_eq!(selected.len(), 1);
    assert!(selected.contains(&0));
}
