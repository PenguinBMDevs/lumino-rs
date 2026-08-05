//! 音符操作测试 —— split / glue / tie

use std::collections::HashSet;

use lumino_note_core::note::Note;

use super::EditorData;

// ── split_note 测试 ──

#[test]
fn test_split_note_success() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 4.0)]);
    let result = data.split_note(0, 2.0);
    assert!(result, "split at middle should succeed");
    assert_eq!(data.current_track_note_count(), 2, "one note becomes two");
    let left = data.get_note_view(0).unwrap();
    assert_eq!(left.tick, 0.0, "left half start tick");
    assert_eq!(left.length, 2.0, "left half length");
    let right = data.get_note_view(1).unwrap();
    assert_eq!(right.tick, 2.0, "right half start tick");
    assert_eq!(right.length, 2.0, "right half length");
}

#[test]
fn test_split_note_invalid_index() {
    let mut data = EditorData::with_f32_notes(0, &[]);
    assert!(!data.split_note(0, 1.0), "empty notes → false");
}

#[test]
fn test_split_note_at_boundary() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 4.0)]);
    assert!(!data.split_note(0, 0.0), "split at start tick = false");
    assert!(!data.split_note(0, 4.0), "split at end tick = false");
}

// ── glue_selected_notes 测试 ──

#[test]
fn test_glue_selected_notes_adjacent() {
    let mut data =
        EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0), Note::new(2.0, 60, 3.0)]);
    let merged = data.glue_selected_notes(&HashSet::from([0, 1]));
    assert_eq!(merged, 1, "should merge one pair");
    assert_eq!(data.current_track_note_count(), 1, "two notes become one");
    let view = data.get_note_view(0).unwrap();
    assert_eq!(view.tick, 0.0);
    assert_eq!(view.length, 5.0, "merged length = sum");
}

#[test]
fn test_glue_selected_notes_non_adjacent() {
    let mut data =
        EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0), Note::new(5.0, 60, 3.0)]);
    let merged = data.glue_selected_notes(&HashSet::from([0, 1]));
    assert_eq!(merged, 0, "non-adjacent notes with gap should not merge");
    assert_eq!(data.current_track_note_count(), 2, "notes unchanged");
}

#[test]
fn test_glue_selected_notes_empty_selection() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0)]);
    assert_eq!(data.glue_selected_notes(&HashSet::new()), 0);
}

// ── tie_selected_notes 测试 ──

#[test]
fn test_tie_selected_notes_same_key_adjacent() {
    let mut data =
        EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0), Note::new(3.0, 60, 3.0)]);
    let tied = data.tie_selected_notes(&HashSet::from([0, 1]));
    assert_eq!(tied, 1, "should tie one pair");
    assert_eq!(data.current_track_note_count(), 2, "notes count unchanged");
    assert_eq!(
        data.get_note_view(0).unwrap().length,
        3.0,
        "first note extends to second's start"
    );
    assert_eq!(
        data.get_note_view(1).unwrap().length,
        3.0,
        "last note unchanged"
    );
}

#[test]
fn test_tie_selected_notes_three_notes() {
    let mut data = EditorData::with_f32_notes(
        0,
        &[
            Note::new(0.0, 60, 2.0),
            Note::new(4.0, 60, 2.0),
            Note::new(8.0, 60, 3.0),
        ],
    );
    let tied = data.tie_selected_notes(&HashSet::from([0, 1, 2]));
    assert_eq!(tied, 2, "should tie two pairs");
    assert_eq!(
        data.get_note_view(0).unwrap().length,
        4.0,
        "first note extends to second"
    );
    assert_eq!(
        data.get_note_view(1).unwrap().length,
        4.0,
        "second note extends to third"
    );
    assert_eq!(
        data.get_note_view(2).unwrap().length,
        3.0,
        "last note unchanged"
    );
}

#[test]
fn test_tie_selected_notes_different_key_still_ties() {
    let mut data =
        EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0), Note::new(3.0, 61, 3.0)]);
    let tied = data.tie_selected_notes(&HashSet::from([0, 1]));
    assert_eq!(tied, 1, "different keys should still tie by tick order");
    assert_eq!(
        data.get_note_view(0).unwrap().length,
        3.0,
        "first note extends to second's start"
    );
    assert_eq!(
        data.get_note_view(1).unwrap().length,
        3.0,
        "last note unchanged"
    );
}

#[test]
fn test_tie_selected_notes_overlapping_notes_not_shortened() {
    // Note 0 starts at 0, ends at 10. Note 1 starts at 3 (overlap).
    // Tie 不应缩短 Note 0，因为重叠不算间隙。
    let mut data =
        EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 10.0), Note::new(3.0, 61, 10.0)]);
    let tied = data.tie_selected_notes(&HashSet::from([0, 1]));
    assert_eq!(tied, 0, "overlapping notes should not be tied");
    assert_eq!(
        data.get_note_view(0).unwrap().length,
        10.0,
        "first note not shortened"
    );
    assert_eq!(
        data.get_note_view(1).unwrap().length,
        10.0,
        "second note unchanged"
    );
}

#[test]
fn test_tie_selected_notes_single_note() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0)]);
    let tied = data.tie_selected_notes(&HashSet::from([0]));
    assert_eq!(tied, 0, "single note cannot tie");
}

#[test]
fn test_tie_selected_notes_empty_selection() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 2.0)]);
    assert_eq!(data.tie_selected_notes(&HashSet::new()), 0);
}

#[test]
fn test_tie_selected_notes_mixed_keys() {
    let mut data = EditorData::with_f32_notes(
        0,
        &[
            Note::new(0.0, 60, 2.0),
            Note::new(3.0, 60, 2.0),
            Note::new(6.0, 61, 2.0),
            Note::new(9.0, 61, 3.0),
        ],
    );
    let tied = data.tie_selected_notes(&HashSet::from([0, 1, 2, 3]));
    // All 4 notes sorted by tick: note0→note1→note2→note3
    // 3 ties: note0→note1, note1→note2, note2→note3
    assert_eq!(tied, 3, "should tie all consecutive pairs by tick order");
    assert_eq!(
        data.get_note_view(0).unwrap().length,
        3.0,
        "note0 extends to note1"
    );
    assert_eq!(
        data.get_note_view(1).unwrap().length,
        3.0,
        "note1 extends to note2"
    );
    assert_eq!(
        data.get_note_view(2).unwrap().length,
        3.0,
        "note2 extends to note3"
    );
    assert_eq!(
        data.get_note_view(3).unwrap().length,
        3.0,
        "last note unchanged"
    );
}

#[test]
fn test_tie_selected_notes_same_tick_group_extends_to_next_tick() {
    // 模拟用户场景：第一小节放置多个不同 Key 的音符，
    // 空一小节，第三小节放置另一组不同 Key 的音符。
    // 选中所有音符后，第一小节的**全部**音符都应延长到第三小节开头。
    let mut data = EditorData::with_f32_notes(
        0,
        &[
            // 第一小节：tick 0，三个不同 Key 的音符，长度 4.0（完整小节）
            Note::new(0.0, 60, 4.0),
            Note::new(0.0, 61, 4.0),
            Note::new(0.0, 62, 4.0),
            // 第三小节：tick 8.0，另一组不同 Key 的音符
            Note::new(8.0, 70, 4.0),
            Note::new(8.0, 71, 4.0),
            Note::new(8.0, 72, 4.0),
        ],
    );

    let tied = data.tie_selected_notes(&HashSet::from([0, 1, 2, 3, 4, 5]));
    assert_eq!(
        tied, 3,
        "all measure-1 notes should extend to measure-3 start"
    );
    assert_eq!(
        data.get_note_view(0).unwrap().length,
        8.0,
        "note0 extends to tick 8"
    );
    assert_eq!(
        data.get_note_view(1).unwrap().length,
        8.0,
        "note1 extends to tick 8"
    );
    assert_eq!(
        data.get_note_view(2).unwrap().length,
        8.0,
        "note2 extends to tick 8"
    );
    assert_eq!(
        data.get_note_view(3).unwrap().length,
        4.0,
        "measure-3 note0 unchanged"
    );
    assert_eq!(
        data.get_note_view(4).unwrap().length,
        4.0,
        "measure-3 note1 unchanged"
    );
    assert_eq!(
        data.get_note_view(5).unwrap().length,
        4.0,
        "measure-3 note2 unchanged"
    );
}
