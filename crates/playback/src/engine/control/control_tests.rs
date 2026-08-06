use super::PlaybackEngine;
use crate::Playback;
use crate::engine::{MidiMessage, NoteEvent};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

use lumino_midi_loader::{MidiDocument, NoteEvent as DocNoteEvent, TrackManager};

#[test]
fn test_event_scheduling() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);

    engine.set_current_track_notes(vec![
        NoteEvent {
            tick: 0.0,
            channel: 0,
            key: 60,
            velocity: 100,
            length: 480.0,
        },
        NoteEvent {
            tick: 480.0,
            channel: 0,
            key: 64,
            velocity: 100,
            length: 480.0,
        },
    ]);

    // 当前轨有 2 个音符 = 4 个事件（NoteOn + NoteOff）
    assert_eq!(engine.event_queue.len(), 4);
}

#[test]
fn test_loop_wrapping_seek_back() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    // 设置从 tick 50 开始的 2 个音符（覆盖循环范围内）
    engine.set_current_track_notes(vec![
        NoteEvent {
            tick: 60.0,
            channel: 0,
            key: 60,
            velocity: 100,
            length: 10.0,
        },
        NoteEvent {
            tick: 90.0,
            channel: 0,
            key: 64,
            velocity: 100,
            length: 10.0,
        },
    ]);

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

    engine.set_current_track_notes(vec![NoteEvent {
        tick: 50.0,
        channel: 0,
        key: 60,
        velocity: 100,
        length: 10.0,
    }]);

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
