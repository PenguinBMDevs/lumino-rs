//! 测试共享辅助（仅 `cfg(test)` 编译）。
//!
//! 收敛散落在 handlers_tests / root_tests / midi tests 中的
//! 25 字段 `MidiDocument` 字面量重复构造——改动字段只需改这一处。

/// 构造最小 2 轨 MidiDocument（音符写入 document，单一权威源）。
pub fn make_test_document() -> lumino_midi_loader::MidiDocument {
    lumino_midi_loader::MidiDocument {
        next_note_id: 1,
        notes: vec![
            lumino_midi_loader::ChunkedList::new(),
            lumino_midi_loader::ChunkedList::new(),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Track 0".into()), Some("Track 1".into())],
        total_ticks: 0,
        track_count: 2,
        tracks: lumino_midi_loader::TrackManager::new(2),
        division: 480,
        track_ports: vec![0, 0],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(2),
    }
}
