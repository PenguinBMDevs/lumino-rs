use super::*;

/// 构造两轨文档：track 0 为空（作为当前轨），track 1 含 3 个交错音符（tick 0/3/6）。
/// 用于验证"其他轨"的静音/独奏过滤。
fn two_track_doc() -> Arc<MidiDocument> {
    Arc::new(MidiDocument {
        next_note_id: 1,
        notes: vec![
            lumino_midi_loader::ChunkedList::new(),
            lumino_midi_loader::ChunkedList::from_sorted(vec![
                DocNoteEvent::new(0, 5, 60, 100, 0),
                DocNoteEvent::new(3, 8, 64, 100, 0),
                DocNoteEvent::new(6, 10, 67, 100, 0),
            ]),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None, None],
        total_ticks: 10,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    })
}

#[test]
fn test_track_should_play_rule() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);

    // 无任何静音/独奏状态：默认全部发声（含越界索引）
    assert!(engine.track_should_play(0));
    assert!(engine.track_should_play(5));

    // 静音 track 1（无独奏）→ 仅 track 1 静音
    engine.set_track_play_states(vec![false, true], vec![false, false]);
    assert!(engine.track_should_play(0));
    assert!(!engine.track_should_play(1));

    // 独奏 track 0 → 仅独奏音轨发声，静音状态被独奏覆盖
    engine.set_track_play_states(vec![false, true], vec![true, false]);
    assert!(engine.track_should_play(0));
    assert!(!engine.track_should_play(1));

    // 多个独奏 → 所有独奏音轨都发声
    engine.set_track_play_states(vec![false, true], vec![true, true]);
    assert!(engine.track_should_play(0));
    assert!(engine.track_should_play(1));
}

#[test]
fn test_current_track_mute_silences_queue() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    engine.set_document(
        doc_with_current_track(vec![
            DocNoteEvent::new(0, 480, 60, 100, 0),
            DocNoteEvent::new(480, 960, 64, 100, 0),
        ]),
        0,
    );
    assert_eq!(engine.event_queue.len(), 4, "未静音时队列应有 4 个事件");

    // 静音当前轨 → 重建后队列应清空
    engine.set_track_play_states(vec![true], vec![false]);
    engine.rebuild_queue_from_current_track(None);
    assert_eq!(engine.event_queue.len(), 0, "当前轨静音后队列应清空");
}

#[test]
fn test_solo_filters_other_track_engine() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    engine.set_document(two_track_doc(), 0);
    // 独奏当前空轨（track 0）→ 其他轨（track 1）不应发声
    engine.set_track_play_states(vec![false, false], vec![true, false]);
    engine.play();
    std::thread::sleep(Duration::from_millis(20));
    let messages = engine.update();

    let note_events: Vec<_> = messages
        .iter()
        .filter(|m| matches!(m, MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. }))
        .collect();
    assert!(
        note_events.is_empty(),
        "独奏未包含的音轨不应发出音符，实际 = {:?}",
        note_events
    );
}

#[test]
fn test_mute_filters_other_track_engine() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    engine.set_document(two_track_doc(), 0);
    // 静音 track 1（无独奏）→ track 1 不应发声
    engine.set_track_play_states(vec![false, true], vec![false, false]);
    engine.play();
    std::thread::sleep(Duration::from_millis(20));
    let messages = engine.update();

    let note_events: Vec<_> = messages
        .iter()
        .filter(|m| matches!(m, MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. }))
        .collect();
    assert!(
        note_events.is_empty(),
        "被静音的音轨不应发出音符，实际 = {:?}",
        note_events
    );
}

#[test]
fn test_solo_plays_only_soloed_track_engine() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    // track 1 设为当前轨（含音符），track 0 为空；独奏 track 1 → 仅 track 1 发声
    let doc = Arc::new(MidiDocument {
        next_note_id: 1,
        notes: vec![
            lumino_midi_loader::ChunkedList::new(),
            lumino_midi_loader::ChunkedList::from_sorted(vec![
                DocNoteEvent::new(0, 5, 60, 100, 0),
                DocNoteEvent::new(3, 8, 64, 100, 0),
                DocNoteEvent::new(6, 10, 67, 100, 0),
            ]),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None, None],
        total_ticks: 10,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    });
    engine.set_document(doc, 1);
    engine.set_track_play_states(vec![false, false], vec![false, true]);
    engine.play();
    std::thread::sleep(Duration::from_millis(20));
    let messages = engine.update();

    let note_events: Vec<_> = messages
        .iter()
        .filter(|m| matches!(m, MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. }))
        .collect();
    assert!(
        !note_events.is_empty(),
        "被独奏的音轨应当发声（验证过滤逻辑方向正确）"
    );
}
