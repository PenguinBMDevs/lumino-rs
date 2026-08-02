use super::*;
use lumino_midi_model::compact::{CompactEvent, EventKind};

fn make_test_document() -> MidiDocument {
    MidiDocument {
        notes: vec![vec![NoteEvent::new(0, 480, 60, 100, 0)]],
        time_signatures: vec![(0, 4, 4)],
        tempo_changes: vec![(0, 120.0)],
        key_signatures: vec![(0, 0, false)],
        control_events: vec![midly::loader::PackedControlEvent::control_change(
            0, 0, 0, 7, 100,
        )],
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Piano".into())],
        total_ticks: 480,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
    }
}

#[test]
fn test_from_midi_document() {
    let doc = make_test_document();
    let project = LuminoProject::from_midi_document(&doc);

    assert_eq!(project.metadata.audio.total_ticks, 480);
    assert_eq!(project.metadata.audio.track_count, 1);
    assert_eq!(project.tempo_changes.len(), 1);
    assert_eq!(project.control_changes.len(), 1);
    assert_eq!(project.loaded_track_count(), 1);

    let track = project.get_track(0).expect("音轨 0 应已加载");
    assert_eq!(track.meta.name, "Piano");
    assert_eq!(track.note_count, 1);
}

#[test]
fn test_to_midi_document_roundtrip() {
    let doc = make_test_document();
    let project = LuminoProject::from_midi_document(&doc);
    let rebuilt = project.to_midi_document().expect("重建 MidiDocument 失败");

    assert_eq!(rebuilt.track_count, 1);
    assert_eq!(rebuilt.total_ticks, 480);
    assert_eq!(rebuilt.notes[0].len(), 1);
    assert_eq!(rebuilt.tempo_changes.len(), 1);
    assert_eq!(rebuilt.control_events.len(), 1);
    assert_eq!(rebuilt.track_names[0], Some("Piano".into()));
}

#[test]
fn test_compact_event_roundtrip() {
    let mut project = LuminoProject::new("Test");
    let events = vec![
        CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
        CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
    ];
    let data = LmtrackData::from_compact_events(
        TrackMeta {
            track_id: 0,
            name: "Test".into(),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 480,
        },
        &events,
    );
    project.add_track(data);

    let doc = project.to_midi_document().expect("重建失败");
    assert_eq!(doc.notes[0].len(), 1);
    assert_eq!(doc.notes[0][0].start_tick, 0);
    assert_eq!(doc.notes[0][0].end_tick, 480);
    assert_eq!(doc.notes[0][0].key, 60);
    assert_eq!(doc.notes[0][0].velocity, 100);
}

#[test]
fn test_to_midi_document_roundtrip_overlapping_notes() {
    let doc = MidiDocument {
        notes: vec![vec![
            NoteEvent::new(0, 480, 60, 100, 0),
            NoteEvent::new(120, 600, 64, 80, 0),
            NoteEvent::new(480, 960, 60, 90, 0),
        ]],
        time_signatures: vec![(0, 4, 4)],
        tempo_changes: vec![(0, 120.0)],
        key_signatures: vec![(0, 0, false)],
        control_events: vec![],
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Piano".into())],
        total_ticks: 960,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
    };
    let project = LuminoProject::from_midi_document(&doc);
    let rebuilt = project.to_midi_document().expect("重叠音符重建失败");

    assert_eq!(rebuilt.notes[0].len(), 3);
    let mut sorted = rebuilt.notes[0].clone();
    sorted.sort_by_key(|n| (n.start_tick, n.key));
    assert_eq!(sorted[0].start_tick, 0);
    assert_eq!(sorted[0].end_tick, 480);
    assert_eq!(sorted[0].key, 60);
    assert_eq!(sorted[0].velocity, 100);
    assert_eq!(sorted[1].start_tick, 120);
    assert_eq!(sorted[1].end_tick, 600);
    assert_eq!(sorted[1].key, 64);
    assert_eq!(sorted[1].velocity, 80);
    assert_eq!(sorted[2].start_tick, 480);
    assert_eq!(sorted[2].end_tick, 960);
    assert_eq!(sorted[2].key, 60);
    assert_eq!(sorted[2].velocity, 90);
}
