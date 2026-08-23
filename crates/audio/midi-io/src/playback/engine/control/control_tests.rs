use super::PlaybackEngine;
use crate::playback::engine::MidiMessage;
use crate::playback::{Playback, PlaybackState};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

use lumino_midi_loader::{MidiDocument, NoteEvent as DocNoteEvent, TrackManager};

/// 构造单轨（track 0 = 当前轨）文档，避免每个测试重复全字段构造。
/// 当前轨统一从 document 流式读取（2026-08 改造后不再有 set_current_track_notes）。
fn doc_with_current_track(notes: Vec<DocNoteEvent>) -> Arc<MidiDocument> {
    let mut max_end = 0u32;
    for n in &notes {
        max_end = max_end.max(n.end_tick);
    }
    Arc::new(MidiDocument {
        next_note_id: 1,
        notes: vec![lumino_midi_loader::ChunkedList::from_sorted(notes)],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![None],
        total_ticks: max_end,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(1),
    })
}

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
fn test_loop_wrapping_seek_back() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    // 设置从 tick 50 开始的 2 个音符（覆盖循环范围内）
    engine.set_document(
        doc_with_current_track(vec![
            DocNoteEvent::new(60, 70, 60, 100, 0),
            DocNoteEvent::new(90, 100, 64, 100, 0),
        ]),
        0,
    );

    // 循环范围 [50, 100)
    engine.set_looping(true);
    engine.set_loop_range(50.0, 100.0);

    // 先播放再暂停来设置初始时间基线（让 Playback 进入 Playing→Paused 状态，积累 paused_microseconds）
    {
        let mut p = playback.lock();
        p.play();
    }
    std::thread::sleep(Duration::from_millis(1));
    {
        let mut p = playback.lock();
        p.pause();
    }

    // seek 到 loop_end 之后（tick = 120）
    engine.seek(120.0);
    // 恢复播放
    engine.play();

    // 调用 update() → 应触发循环回绕
    let _messages = engine.update();

    // current_tick 应回到 loop_start (50) 附近
    let new_tick = engine.current_tick();
    assert!(
        (48.0..=52.0).contains(&new_tick),
        "循环回绕后 current_tick 应接近 loop_start(50)，实际 = {}",
        new_tick,
    );

    // last_processed_tick 也应被重置
    assert!(
        (48.0..=52.0).contains(&engine.last_processed_tick),
        "last_processed_tick 应接近 loop_start(50)，实际 = {}",
        engine.last_processed_tick,
    );

    // 事件队列应被重建，包含循环起点后的事件
    assert!(!engine.event_queue.is_empty(), "回绕后事件队列不应为空",);

    // 检查 event_queue 中的事件 tick >= loop_start
    let events: Vec<_> = engine.event_queue.iter().collect();
    // BinaryHeap 是最大堆，注意 tick 小的优先级高
    let min_event_tick = events.iter().map(|event| event.tick).min_by(|a, b| {
        a.partial_cmp(b)
            .expect("f64 的 partial_cmp 应返回 Some，因为 tick 不是 NaN")
    });
    assert!(
        min_event_tick.is_some()
            && min_event_tick.expect("事件队列不应为空，至少应有一个事件") >= 50.0,
        "队列中最先要播放的事件 tick 应 >= loop_start(50)，实际 = {:?}",
        min_event_tick,
    );

    // 第二次 update() 不应再次触发循环回绕（tick 还在范围内）
    let _messages2 = engine.update();
    let tick_after_second = engine.current_tick();
    assert!(
        tick_after_second >= 48.0,
        "第二次 update 后 tick 不应跳回 0，实际 = {}",
        tick_after_second,
    );
}

#[test]
fn test_loop_wrapping_disabled() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    engine.set_document(
        doc_with_current_track(vec![
            DocNoteEvent::new(50, 60, 60, 100, 0),
            // 扩展一条更长音符，使轨尾标 (end_tick=200) 超过 seek 位置 150，
            // 验证"禁用循环时不回绕"的同时不触发轨尾标自动停止。
            DocNoteEvent::new(90, 200, 64, 100, 0),
        ]),
        0,
    );

    // 设置循环范围但未启用 looping
    engine.set_looping(false);
    engine.set_loop_range(50.0, 100.0);

    // 先播放再暂停设基线
    {
        let mut p = playback.lock();
        p.play();
    }
    std::thread::sleep(Duration::from_millis(1));
    {
        let mut p = playback.lock();
        p.pause();
    }

    engine.seek(150.0);
    engine.play();
    let _messages = engine.update();

    let tick = engine.current_tick();
    // 没有回绕，tick 应保持在 150 附近
    assert!(
        (145.0..=155.0).contains(&tick),
        "禁用循环后 tick 应保持在 seek 位置 (150)，实际 = {}",
        tick,
    );
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

    // 让时间推进足够覆盖全部音符（约 10 tick ≈ 10 ms）
    std::thread::sleep(Duration::from_millis(20));
    let messages = engine.update();

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
fn test_playback_stops_at_track_end_marker() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    // 当前轨 2 个音符：tick 0/480，长度 480 → 轨尾标 = 960
    engine.set_document(
        doc_with_current_track(vec![
            DocNoteEvent::new(0, 480, 60, 100, 0),
            DocNoteEvent::new(480, 960, 64, 100, 0),
        ]),
        0,
    );

    engine.play();
    // 跳到轨尾标（最后音符结束 tick = 960）处，模拟播放到达终点
    engine.seek_playback(960.0);

    let _messages = engine.update();

    assert_eq!(
        engine.state(),
        PlaybackState::Stopped,
        "播放到达轨尾标 (tracks_max_end_tick) 应自动停止"
    );
    assert_eq!(
        engine.current_tick(),
        0.0,
        "自动停止后 current_tick 应复位到起点（与手动 Stop 语义一致）"
    );
}

#[test]
fn test_playback_stops_at_track_end_marker_when_extended_by_edit() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    // 初始轨尾标 = 480
    engine.set_document(
        doc_with_current_track(vec![DocNoteEvent::new(0, 480, 60, 100, 0)]),
        0,
    );
    engine.play();
    engine.seek_playback(480.0);
    let _ = engine.update();
    assert_eq!(
        engine.state(),
        PlaybackState::Stopped,
        "到达初始轨尾标应停止"
    );

    // 编辑：在最后音符后追加一颗更长音符，轨尾标扩展为 1200。
    // 重新发送 document 快照（模拟 update_playback_notes 的 set_document）。
    let extended = doc_with_current_track(vec![
        DocNoteEvent::new(0, 480, 60, 100, 0),
        DocNoteEvent::new(600, 1200, 64, 100, 0),
    ]);
    engine.set_document(extended, 0);
    // 从 480 继续播放（未越新轨尾标）
    engine.seek_playback(480.0);
    engine.play();
    // 越过原轨尾标 480、但仍在 [480, 1200) 内，不应停止
    engine.seek_playback(700.0);
    let _ = engine.update();
    assert_eq!(
        engine.state(),
        PlaybackState::Playing,
        "轨尾标随编辑扩展后，到达旧终点不应提前停止"
    );
    // 越过新轨尾标 1200 应停止
    engine.seek_playback(1200.0);
    let _ = engine.update();
    assert_eq!(
        engine.state(),
        PlaybackState::Stopped,
        "编辑扩展后的新轨尾标应作为停止点"
    );
}

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
