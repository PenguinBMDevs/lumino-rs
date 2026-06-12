use super::*;
use crate::playback::Playback;
use std::time::Duration;

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
        new_tick >= 48.0 && new_tick <= 52.0,
        "循环回绕后 current_tick 应接近 loop_start(50)，实际 = {}",
        new_tick,
    );

    // last_processed_tick 也应被重置
    assert!(
        engine.last_processed_tick >= 48.0 && engine.last_processed_tick <= 52.0,
        "last_processed_tick 应接近 loop_start(50)，实际 = {}",
        engine.last_processed_tick,
    );

    // 事件队列应被重建，包含循环起点后的事件
    assert!(!engine.event_queue.is_empty(), "回绕后事件队列不应为空",);

    // 检查 event_queue 中的事件 tick >= loop_start
    let events: Vec<_> = engine.event_queue.iter().collect();
    // BinaryHeap 是最大堆，注意 tick 小的优先级高
    let min_event_tick = events
        .iter()
        .map(|e| e.tick)
        .min_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        min_event_tick.is_some() && min_event_tick.unwrap() >= 50.0,
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
    // 没有回绕，tick 应在 150 附近
    assert!(
        tick >= 145.0 && tick <= 155.0,
        "禁用循环后 tick 应保持在 seek 位置 (150)，实际 = {}",
        tick,
    );
}
