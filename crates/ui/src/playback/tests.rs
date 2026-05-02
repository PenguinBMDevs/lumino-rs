//! 播放器模块单元测试

use super::core::Playback;
use super::engine::PlaybackState;
use super::tempo::{bpm_from_tempo, tempo_from_bpm, TempoChange};
use super::timeline::Timeline;

#[test]
fn test_tempo_conversion() {
    let bpm = 120.0;
    let tempo = tempo_from_bpm(bpm);
    assert_eq!(tempo, 500_000);
    assert_eq!(bpm_from_tempo(tempo), bpm);
}

#[test]
fn test_timeline_single_tempo() {
    let timeline = Timeline::new(480);
    // 默认120 BPM = 500000 微秒/拍
    // 480 ticks = 1拍 = 500000微秒
    let microseconds = timeline.tick_to_microseconds(480.0);
    assert_eq!(microseconds, 500_000);

    let tick = timeline.microseconds_to_tick(500_000);
    assert!((tick - 480.0).abs() < 0.1);
}

#[test]
fn test_timeline_tempo_changes() {
    let mut timeline = Timeline::new(480);
    timeline.set_tempo_changes(vec![
        TempoChange::from_bpm(0.0, 120.0),   // 0-480: 120 BPM
        TempoChange::from_bpm(480.0, 240.0), // 480+: 240 BPM
    ]);

    // 前480 ticks: 120 BPM = 500000微秒
    assert_eq!(timeline.tick_to_microseconds(480.0), 500_000);

    // 480-960: 240 BPM = 250000微秒
    // 总共: 500000 + 250000 = 750000
    assert_eq!(timeline.tick_to_microseconds(960.0), 750_000);
}

#[test]
fn test_playback_state() {
    let mut playback = Playback::new(480);
    assert_eq!(playback.state(), PlaybackState::Stopped);

    playback.play();
    assert_eq!(playback.state(), PlaybackState::Playing);

    playback.pause();
    assert_eq!(playback.state(), PlaybackState::Paused);

    playback.stop();
    assert_eq!(playback.state(), PlaybackState::Stopped);
}
