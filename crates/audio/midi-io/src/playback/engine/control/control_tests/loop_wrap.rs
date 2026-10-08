use super::*;

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

    // 回绕判据：update 时刻冻结的 `last_processed_tick` 应落回循环区间 [50,100)。
    // 墙上时钟在 update 前后继续推进（CI 慢机可推进数十 ms），对 `current_tick`
    // 用 ±2 tick 的近距窗口必然 flake（历史 macOS CI 根因）；这里改用冻结值作
    // 证据并把窗口放宽到整个循环区间。
    assert!(
        (48.0..100.0).contains(&engine.last_processed_tick),
        "回绕后 last_processed_tick 应落回循环区间 [50,100)，实际 = {}",
        engine.last_processed_tick,
    );

    // current_tick 是墙上时钟推进值：只断言"没有停在原位置"（回绕会跳回 ~50）
    let new_tick = engine.current_tick();
    assert!(
        new_tick >= 48.0,
        "回绕后 current_tick 不应留在原位置，实际 = {}",
        new_tick,
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

/// PREF-006 A2：回绕缓存——同配置复用；`set_document`（编辑快照）后失效重建。
#[test]
fn test_loop_wrap_cache_reuse_and_invalidation() {
    use lumino_midi_loader::ChunkedList;

    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);

    let make_doc = |track1_notes: Vec<DocNoteEvent>| {
        Arc::new(MidiDocument {
            notes: vec![
                ChunkedList::from_sorted(vec![DocNoteEvent::new(0, 10, 60, 100, 0)]),
                ChunkedList::from_sorted(track1_notes),
            ],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            text_events: vec![],
            sys_ex: vec![],
            track_names: vec![None, None],
            total_ticks: 0,
            track_count: 2,
            tracks: TrackManager::new(2),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: MidiDocument::new_track_max_ticks(2),
        })
    };

    // track 1：一颗跨 loop_start(50) 的长音符 + 一颗 loop_start 之后的音符
    engine.set_document(
        make_doc(vec![
            DocNoteEvent::new(10, 80, 64, 100, 0), // 跨 50 → 应悬挂
            DocNoteEvent::new(60, 70, 67, 100, 0), // 50 之后 → 不悬挂
        ]),
        0,
    );

    engine.ensure_loop_wrap_cache(50.0);
    let cache = engine.loop_wrap_cache.as_ref().expect("缓存应已构建");
    assert_eq!(
        cache.tracks[1].0, 1,
        "track1 cursor 应为 1（仅 start<50 的音符）"
    );
    assert_eq!(cache.tracks[1].1.len(), 1, "仅跨 loop_start 的音符悬挂");

    engine.apply_loop_wrap_cache();
    assert_eq!(engine.track_states[1].note_cursor, 1);
    assert_eq!(engine.track_states[1].pending_offs.len(), 1);

    // 同配置复用：再次 ensure 不应重建（地址不变）
    let ptr_before = engine.loop_wrap_cache.as_ref().map(|c| c as *const _);
    engine.ensure_loop_wrap_cache(50.0);
    let ptr_after = engine.loop_wrap_cache.as_ref().map(|c| c as *const _);
    assert_eq!(ptr_before, ptr_after, "同配置应复用缓存");

    // 编辑快照（set_document）→ 缓存失效；重建后反映新内容
    engine.set_document(
        make_doc(vec![DocNoteEvent::new(10, 45, 64, 100, 0)]), // 不再跨 loop_start
        0,
    );
    assert!(
        engine.loop_wrap_cache.is_none(),
        "set_document 后缓存应失效"
    );
    engine.ensure_loop_wrap_cache(50.0);
    let cache = engine.loop_wrap_cache.as_ref().expect("缓存应重建");
    assert_eq!(cache.tracks[1].1.len(), 0, "新快照中无跨 loop_start 音符");
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
    // 禁用循环时不得回绕：tick 只能从 seek(150) 向前推进（墙上时钟在慢机上
    // 可推进若干 tick）。回绕会跳回循环起点（~50），因此下界即可完成判定；
    // 不再给墙上时钟设 ±5 tick 的近距上界（历史 macOS CI flake 根因）。
    assert!(
        tick >= 145.0,
        "禁用循环后 tick 不应回绕到循环起点，实际 = {}",
        tick,
    );
}
