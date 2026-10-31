//! 编辑层单元测试。

use std::sync::Arc;

use lumino_midi_loader::MidiDocument;

use super::{Drag, Editor, Tool};
use crate::{model::TrackIndex, render::density::DensityPyramid};

fn fixture() -> (Arc<MidiDocument>, Arc<TrackIndex>, DensityPyramid) {
    let mut document = MidiDocument::empty_with_tracks(2, 1920);
    document.insert_note(0, lumino_midi_loader::NoteEvent::new(0, 120, 60, 100, 0));
    document.insert_note(0, lumino_midi_loader::NoteEvent::new(480, 600, 62, 100, 0));
    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let density = DensityPyramid::build(&document, 1);
    (document, index, density)
}

#[test]
fn insert_delete_undo_redo_round_trip() {
    let (mut document, mut index, mut density) = fixture();
    let mut editor = Editor::new(1920);
    editor.insert_at(&mut document, &mut index, &mut density, 0, 960, 64);
    assert_eq!(document.track_note_count(0), 3);
    assert_eq!(editor.selection_count(), 1);

    assert!(editor.delete_selected(&mut document, &mut index, &mut density, 0));
    assert_eq!(document.track_note_count(0), 2);
    assert!(editor.undo(&mut document, &mut index, &mut density));
    assert_eq!(document.track_note_count(0), 3);
    assert!(editor.redo(&mut document, &mut index, &mut density));
    assert_eq!(document.track_note_count(0), 2);
}

#[test]
fn undo_redo_replays_density_without_full_dirty() {
    let (mut document, mut index, mut density) = fixture();
    let mut editor = Editor::new(1920);
    editor.insert_at(&mut document, &mut index, &mut density, 0, 960, 64);
    assert!(!editor.density_dirty, "插入后应只标记覆盖脏区");

    assert!(editor.undo(&mut document, &mut index, &mut density));
    assert!(!editor.density_dirty, "撤销不得要求整建密度");
    assert!(
        density.rebuild_dirty_coverage(&document, &index),
        "撤销后覆盖层必须能局部重建"
    );

    assert!(editor.redo(&mut document, &mut index, &mut density));
    assert!(!editor.density_dirty, "重做不得要求整建密度");
    assert!(density.rebuild_dirty_coverage(&document, &index));
}

#[test]
fn hit_and_box_select() {
    let (document, index, _density) = fixture();
    let hit = Editor::hit_test(&document, &index, 0, 60, 60);
    assert!(hit.is_some());
    assert!(Editor::hit_test(&document, &index, 0, 200, 60).is_none());

    let mut editor = Editor::new(1920);
    editor.box_select(&document, &index, 0, 0, 1000, 0, 127, false);
    assert_eq!(editor.selection_count(), 2);
}

#[test]
fn move_commits_values_and_index() {
    let (mut document, mut index, mut density) = fixture();
    let mut editor = Editor::new(1920);
    editor.tool = Tool::Insert;
    assert_eq!(editor.tool_name(), "insert");
    editor.click_select(0, false, false);
    editor.begin_move(&document, 0, 0, 60);
    editor.update_drag(480, 62);
    if let Some(Drag::Move {
        delta_tick,
        delta_key,
        ..
    }) = &editor.drag
    {
        assert_eq!(*delta_tick, 480);
        assert_eq!(*delta_key, 2);
    }
    assert!(editor.commit_move(&mut document, &mut index, &mut density, 0));
    let notes = document.track_notes(0);
    assert_eq!(notes.len(), 2);
    assert_eq!(
        notes.get(0).map(|note| (note.start_tick, note.key)),
        Some((480, 62))
    );
}
