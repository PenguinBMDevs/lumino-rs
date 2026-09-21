use super::*;

#[test]
fn test_event_scheduling() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);

    // 当前轨（track 0）2 个音符：tick 0/480，长度 480
    engine.set_document(
        doc_with_current_track(vec![
            DocNoteEvent::new(0, 480, 60, 100, 0),
            DocNoteEvent::new(480, 960, 64, 100, 0),
        ]),
        0,
    );

    // 当前轨有 2 个音符 = 4 个事件（NoteOn + NoteOff）
    assert_eq!(engine.event_queue.len(), 4);
}

#[test]
fn test_document_streaming_emits_events_in_order() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    // 构造一个两轨文档：track 0 为当前轨（空），track 1 为其他轨。
    // 其他轨的音符故意交错，验证 NoteOn/NoteOff 按时间顺序合并输出。
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
        sys_ex: vec![],
        track_names: vec![None, None],
        total_ticks: 10,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],

        track_max_end_ticks: vec![],
    });

    engine.set_document(doc, 0);
    engine.play();

    // 直接以固定 tick 驱动「多轨流式合并」路径，锁定「按时间顺序合并」的语义：
    // 生产入口 `update()` 含「迟到 > LATE_NOTE_SKIP_SECS（150ms）即跳过」的
    // wall-clock 语义，CI runner 卡顿会把全部音符误判为迟到（macOS 实测返回空
    // 事件列表），故此测试不走 wall-clock，改用固定 current_tick=10 / late_bound=0。
    let mut messages = Vec::new();
    engine.process_other_tracks(10.0, 0.0, &mut messages);

    // 收集所有 NoteOn/NoteOff 的 key 与类型，验证时间顺序
    let event_keys: Vec<_> = messages
        .iter()
        .filter_map(|msg| match msg {
            MidiMessage::NoteOn { key, .. } => Some(("on", *key)),
            MidiMessage::NoteOff { key, .. } => Some(("off", *key)),
            _ => None,
        })
        .collect();

    // 期望顺序：0:on(60), 3:on(64), 5:off(60), 6:on(67), 8:off(64), 10:off(67)
    let expected = vec![
        ("on", 60),
        ("on", 64),
        ("off", 60),
        ("on", 67),
        ("off", 64),
        ("off", 67),
    ];
    assert_eq!(
        event_keys, expected,
        "从 MidiDocument 直接流式读取应按时序发出 NoteOn/NoteOff"
    );
}

#[test]
fn late_notes_are_skipped_not_replayed() {
    // 模拟播放线程停顿/跳变：last_processed_tick 停在 0，而播放头已跳到 10000 tick。
    // 旧实现会把 [0, 10000) 区间内所有音符补发；新语义：迟到超过 150ms 的音符直接跳过。
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    let doc = Arc::new(MidiDocument {
        next_note_id: 1,
        notes: vec![
            lumino_midi_loader::ChunkedList::new(), // track 0：当前轨（空）
            lumino_midi_loader::ChunkedList::from_sorted(vec![
                // 过时音符：tick 10（远早于迟到边界）
                DocNoteEvent::new(10, 20, 60, 100, 0),
                // 新鲜音符：tick 9900（边界 10000-144=9856 之后）
                DocNoteEvent::new(9900, 9990, 64, 100, 0),
            ]),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![None, None],
        total_ticks: 10000,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    });

    engine.set_document(doc, 0);
    engine.play();
    engine.last_processed_tick = 0.0; // 模拟停顿：上一块处理位置远落后于播放头
    playback.lock().seek(10000.0); // 播放头跳到 10000 tick（≈10.4s @120BPM/480）

    let messages = engine.update();
    let note_on_keys: Vec<u8> = messages
        .iter()
        .filter_map(|msg| match msg {
            MidiMessage::NoteOn { key, .. } => Some(*key),
            _ => None,
        })
        .collect();

    assert_eq!(
        note_on_keys,
        vec![64],
        "迟到音符（key=60）必须被跳过，只允许边界内的新音符（key=64）发声"
    );
}
