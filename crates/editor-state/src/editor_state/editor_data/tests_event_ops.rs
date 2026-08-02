//! 事件浏览器 CRUD 操作测试

use std::collections::HashSet;

use lumino_note_core::event::{
    AutomationTarget, ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent,
    ScaleType, SegmentShape, TimeSignatureEvent,
};

use super::EditorData;

#[test]
fn test_time_sig_crud() {
    let mut data = EditorData::new();
    data.set_time_sig_event(0, 4, 4);
    data.set_time_sig_event(960, 3, 4);
    data.insert_time_sig_event(480);

    assert_eq!(data.time_signatures.len(), 3);
    assert_eq!(data.time_signatures[0], (0, 4, 4));
    assert_eq!(data.time_signatures[1], (480, 4, 4));
    assert_eq!(data.time_signatures[2], (960, 3, 4));

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_time_sig_events(&ticks);
    assert_eq!(data.time_signatures.len(), 2);
    assert_eq!(data.time_signatures[0], (480, 4, 4));
}

#[test]
fn test_key_sig_crud() {
    let mut data = EditorData::new();
    data.set_key_sig_event(0, 0, ScaleType::Major);
    data.set_key_sig_event(960, 7, ScaleType::Minor);
    data.insert_key_sig_event(480);

    assert_eq!(data.key_signatures.len(), 3);
    assert_eq!(data.key_signatures[0].tick, 0);
    assert_eq!(data.key_signatures[1].tick, 480);
    assert_eq!(data.key_signatures[2].scale, ScaleType::Minor);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_key_sig_events(&ticks);
    assert_eq!(data.key_signatures.len(), 2);
}

#[test]
fn test_marker_crud() {
    let mut data = EditorData::new();
    data.set_marker_event(0, "Intro".into());
    data.insert_marker_event(480);

    assert_eq!(data.markers.len(), 2);
    assert_eq!(data.markers[0].text, "Intro");
    assert_eq!(data.markers[1].text, "New");

    let mut ticks = HashSet::new();
    ticks.insert(480);
    data.delete_marker_events(&ticks);
    assert_eq!(data.markers.len(), 1);
}

#[test]
fn test_lyrics_crud() {
    let mut data = EditorData::new();
    data.set_lyrics_event(0, "La".into());
    data.insert_lyrics_event(480);

    assert_eq!(data.lyrics.len(), 2);
    assert_eq!(data.lyrics[0].text, "La");
    assert_eq!(data.lyrics[1].text, "");

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_lyrics_events(&ticks);
    assert_eq!(data.lyrics.len(), 1);
}

#[test]
fn test_chord_crud() {
    let mut data = EditorData::new();
    data.set_chord_event(0, "C".into());
    data.insert_chord_event(480);

    assert_eq!(data.chords.len(), 2);
    assert_eq!(data.chords[0].text, "C");
    assert_eq!(data.chords[1].text, "");

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_chord_events(&ticks);
    assert_eq!(data.chords.len(), 1);
}

#[test]
fn test_program_change_crud() {
    let mut data = EditorData::new();
    data.set_program_change_event(0, 5);
    data.insert_program_change_event(480);

    assert_eq!(data.program_changes.len(), 2);
    assert_eq!(data.program_changes[0].program, 5);
    assert_eq!(data.program_changes[1].program, 0);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_program_change_events(&ticks);
    assert_eq!(data.program_changes.len(), 1);
}

#[test]
fn test_automation_cc_crud() {
    let mut data = EditorData::new();
    data.set_automation_event(1, AutomationTarget::Cc(7), 0, 100.0, SegmentShape::Step);

    let idx = data
        .find_automation_lane(
            1,
            &lumino_note_core::automation::AutomationTarget::CC { controller: 7 },
        )
        .expect("应存在 volume lane");
    assert_eq!(data.automation_lanes[idx].events.len(), 1);
    assert_eq!(data.automation_lanes[idx].events[0].value, 100);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_automation_events(1, &AutomationTarget::Cc(7), &ticks);
    assert!(data.automation_lanes[idx].events.is_empty());

    data.insert_automation_event(1, &AutomationTarget::Cc(7), 480);
    assert_eq!(data.automation_lanes[idx].events.len(), 1);
    assert_eq!(data.automation_lanes[idx].events[0].value, 0);
}

#[test]
fn test_automation_tempo() {
    let mut data = EditorData::new();
    data.set_automation_event(0, AutomationTarget::Tempo, 0, 140.0, SegmentShape::Step);
    assert_eq!(data.tempo_points.len(), 1);
    assert!((data.tempo_points[0].bpm - 140.0).abs() < f64::EPSILON);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_automation_events(0, &AutomationTarget::Tempo, &ticks);
    assert!(data.tempo_points.is_empty());
}

#[test]
fn test_insert_note_on_nonzero_track() {
    let mut data = EditorData::new();
    data.current_track = 0;
    assert!(data.insert_note_at_tick(100.0).is_none());

    data.current_track = 1;
    let note = data.insert_note_at_tick(100.0).expect("应成功插入音符");
    assert_eq!(note.tick, 100.0);
    assert_eq!(note.key, 60);
    assert_eq!(note.length, 480.0);
    assert_eq!(note.velocity, 100);
    assert_eq!(data.notes.len(), 1);
}

#[test]
fn test_delete_notes_at_ticks() {
    let mut data = EditorData::new();
    data.current_track = 1;
    data.insert_note_at_tick(100.0);
    data.insert_note_at_tick(200.0);
    data.insert_note_at_tick(300.0);
    assert_eq!(data.notes.len(), 3);

    let mut ticks = HashSet::new();
    ticks.insert(100);
    ticks.insert(300);
    data.delete_notes_at_ticks(&ticks);

    assert_eq!(data.notes.len(), 1);
    assert_eq!(data.notes[0].tick, 200.0);
}

#[test]
fn test_reset_clears_event_fields() {
    let mut data = EditorData::new();
    data.set_marker_event(0, "A".into());
    data.set_key_sig_event(0, 0, ScaleType::Major);
    data.reset();
    assert!(data.markers.is_empty());
    assert!(data.key_signatures.is_empty());
    assert!(data.lyrics.is_empty());
    assert!(data.chords.is_empty());
    assert!(data.program_changes.is_empty());
}
