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
