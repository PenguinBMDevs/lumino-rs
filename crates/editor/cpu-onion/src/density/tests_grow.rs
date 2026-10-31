//! 密度/覆盖布局扩容单测（编辑越过既有右界，不做整份重建）。

use lumino_midi_model::{MidiDocument, NoteEvent};

use super::DensityPyramid;
use crate::TrackIndex;

fn sample_document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(0, 2000, 125, 100, 0));
    document
}

#[test]
fn range_rebuild_matches_full_after_direct_edit() {
    let before = sample_document();
    let mut after = before.clone();
    if let Some(note) = after.track_notes_mut(1).and_then(|track| track.get_mut(0)) {
        note.end_tick = 2600;
    }
    let index = TrackIndex::build(&after);
    let mut pyramid = DensityPyramid::build(&before, 1, 128);
    assert!(
        pyramid.rebuild_dirty_ranges(&after, &index, &[(0, 2600)]),
        "有区间时必须局部重建"
    );
    assert_eq!(pyramid.levels[0].cells[125], 1, "tick 0 / key 125 必须保留");
    let rebuilt = DensityPyramid::build(&after, 1, 128);
    for level in 0..pyramid.levels.len() {
        assert_eq!(
            pyramid.levels[level].cells, rebuilt.levels[level].cells,
            "区间重建后第 {level} 层计数必须与整建一致"
        );
    }
    assert!(pyramid.coverage_valid());
}

#[test]
fn apply_note_outside_layout_grows_without_rebuild() {
    let mut edited = sample_document();
    let note = NoteEvent::new(3000, 3200, 60, 100, 0);
    edited.insert_note(1, note);
    let index = TrackIndex::build(&edited);

    let mut incremental = DensityPyramid::build(&sample_document(), 1, 128);
    assert!(
        incremental.apply_note_range(note.start_tick, note.end_tick, note.key.into(), 1),
        "越过既有右界只应扩容，不得要求整建"
    );
    assert!(incremental.rebuild_dirty_coverage(&edited, &index));

    let rebuilt = DensityPyramid::build(&edited, 1, 128);
    assert_eq!(incremental.total_ticks(), rebuilt.total_ticks());
    for level in 0..incremental.levels.len() {
        assert_eq!(
            incremental.levels[level].cells, rebuilt.levels[level].cells,
            "扩容后第 {level} 层必须与整建一致"
        );
    }
}
