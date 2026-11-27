use super::*;

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

/// 回归：播到尾自动停止后再次起播必须有声。
///
/// BUG 现象：演奏指示线回到开头（`current_tick=0`），但音频引擎进度
/// （游标/队列）仍停在尾部，下次起播无声。
/// 自动停止必须走引擎级 `stop()` 全量复位，且下次从停止态起播时重建队列。
#[test]
fn test_replay_after_auto_stop_emits_sound() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    engine.set_document(
        doc_with_current_track(vec![
            DocNoteEvent::new(0, 480, 60, 100, 0),
            DocNoteEvent::new(480, 960, 64, 100, 0),
        ]),
        0,
    );
    engine.play();
    engine.seek_playback(960.0);
    let _ = engine.update();
    assert_eq!(engine.state(), PlaybackState::Stopped);
    assert_eq!(engine.current_tick(), 0.0);
    // 引擎进度必须复位到开头
    assert_eq!(
        engine.last_processed_tick, 0.0,
        "自动停止后 last_processed_tick 应复位到 0"
    );
    assert_eq!(
        engine.control_event_cursor, 0,
        "自动停止后控制事件游标应复位到 0"
    );
    assert_eq!(
        engine.midi_event_cursor, 0,
        "自动停止后 MIDI 事件游标应复位到 0"
    );

    // 再次起播：当前轨游标必须重定位（PREF-006 A1 流式模型无预建队列），
    // 否则后续播放无声
    engine.play();
    assert_eq!(engine.state(), PlaybackState::Playing);
    assert_eq!(
        engine.track_states[0].note_cursor, 0,
        "自动停止后重播应重定位当前轨游标到开头，否则无声"
    );
    let tick = engine.current_tick();
    assert!(tick < 50.0, "重播起播 tick 应在开头附近，实际 = {}", tick,);
}

/// 回归：停止态下拖动指示线（seek）后按播放，应从 seek 位置起播而非归零。
#[test]
fn test_play_preserves_seek_while_stopped() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    engine.set_document(
        doc_with_current_track(vec![
            DocNoteEvent::new(0, 480, 60, 100, 0),
            DocNoteEvent::new(480, 960, 64, 100, 0),
        ]),
        0,
    );
    // 停止态下 seek 到 480（模拟拖动演奏指示线后按播放）
    engine.seek(480.0);
    engine.play();
    let tick = engine.current_tick();
    assert!(
        (470.0..=490.0).contains(&tick),
        "停止态 seek 后起播应保留 seek 位置（480 附近），实际 = {}",
        tick,
    );
    assert!(
        (479.0..=481.0).contains(&engine.last_processed_tick),
        "起播后 last_processed_tick 应与 seek 位置对齐，实际 = {}",
        engine.last_processed_tick,
    );
}
