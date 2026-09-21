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
