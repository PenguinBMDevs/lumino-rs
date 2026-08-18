//! 播放器模块单元测试

use crate::playback::PlaybackState;
use crate::playback::core::Playback;
use crate::playback::tempo::{TempoChange, bpm_from_tempo, tempo_from_bpm};
use crate::playback::timeline::Timeline;

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

/// 验证多段 tempo 下 `tick_to_microseconds` / `microseconds_to_tick` 互逆，
/// 且 `microseconds_to_tick` 在大 target 下仍然精确——这是修复"播放越久越慢"的回归保护。
#[test]
fn test_timeline_multi_segment_round_trip() {
    let mut timeline = Timeline::new(480);
    timeline.set_tempo_changes(vec![
        TempoChange::from_bpm(0.0, 120.0),   // 段0: 0..480
        TempoChange::from_bpm(480.0, 240.0), // 段1: 480..960
        TempoChange::from_bpm(960.0, 60.0),  // 段2: 960..+∞
    ]);

    // 段边界：480→500000µs, 960→750000µs
    assert_eq!(timeline.tick_to_microseconds(480.0), 500_000);
    assert_eq!(timeline.tick_to_microseconds(960.0), 750_000);

    // 段内任意点互逆（段0中部 tick=240 → 250000µs）
    assert_eq!(timeline.tick_to_microseconds(240.0), 250_000);
    assert!((timeline.microseconds_to_tick(250_000) - 240.0).abs() < 0.1);

    // 段1中部 tick=720 → 500000 + 250000/2 = 625000µs
    assert_eq!(timeline.tick_to_microseconds(720.0), 625_000);
    assert!((timeline.microseconds_to_tick(625_000) - 720.0).abs() < 0.1);

    // 段2（60 BPM = 1000000µs/拍）：tick=1440 → 750000 + 480/480*1000000 = 1750000µs
    assert_eq!(timeline.tick_to_microseconds(1440.0), 1_750_000);
    assert!((timeline.microseconds_to_tick(1_750_000) - 1440.0).abs() < 0.1);

    // 大 target（模拟播放很久后的查询）—— O(log N) 应保持精确
    let big_tick = 1_000_000.0_f32;
    let big_micros = timeline.tick_to_microseconds(big_tick);
    let back = timeline.microseconds_to_tick(big_micros);
    assert!(
        (back - big_tick).abs() < 1.0,
        "大 target 往返不一致: tick={big_tick} → micros={big_micros} → back={back}"
    );
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
