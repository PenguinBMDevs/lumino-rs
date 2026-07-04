//! 播放器模块单元测试

use crate::PlaybackState;
use crate::core::Playback;
use crate::manager::PlaybackManager;
use crate::tempo::{TempoChange, bpm_from_tempo, tempo_from_bpm};
use crate::timeline::Timeline;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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

/// ─────────────────────────────────────────────────────────────
/// PlaybackManager 音频链路变更单元测试
/// ─────────────────────────────────────────────────────────────

/// 模拟 MIDI 输出，通过 channel 收集收到的命令
struct SpyOutput {
    note_on_tx: mpsc::Sender<(u8, u8, u8)>,
}

impl SpyOutput {
    fn new() -> (Self, mpsc::Receiver<(u8, u8, u8)>) {
        let (tx, rx) = mpsc::channel();
        (Self { note_on_tx: tx }, rx)
    }
}

impl lumino_midi_io::OutputConnection for SpyOutput {
    fn note_on(
        &mut self,
        ch: u8,
        key: u8,
        vel: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        let _ = self.note_on_tx.send((ch, key, vel));
        Ok(())
    }
    fn send_raw(&mut self, _data: [u8; 3]) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn close(self: Box<Self>) {}
}

/// 场景1：PlaybackManager 替换 MIDI 输出后，新输出接收事件
///
/// 关键设计：使用多条跨不同 tick 的音符，确保 swap 前后都有未处理的事件。
/// tick 0 的事件在第一次 update() 中被消耗并通过输出 A 发送，
/// tick 480 和 tick 960 的事件在 swap 后才到达，应通过输出 B 发送。
#[test]
fn test_manager_replaces_midi_output() {
    let mut manager = PlaybackManager::new(480);

    // 三条音符：tick 0, 480, 960（各自相距 1 拍）
    manager.set_current_track_notes(vec![
        crate::NoteEvent {
            tick: 0.0,
            channel: 0,
            key: 60,
            velocity: 100,
            length: 240.0,
        },
        crate::NoteEvent {
            tick: 480.0,
            channel: 0,
            key: 64,
            velocity: 100,
            length: 240.0,
        },
        crate::NoteEvent {
            tick: 960.0,
            channel: 0,
            key: 67,
            velocity: 100,
            length: 240.0,
        },
    ]);

    // ── 输出 A ──
    let (output_a, rx_a) = SpyOutput::new();
    manager.set_midi_output(Box::new(output_a));
    manager.play();

    // 等待输出 A 收到 note_on (tick 0 的音符先到达)
    let a_msg = rx_a
        .recv_timeout(Duration::from_secs(2))
        .expect("输出 A 应在 2s 内收到 note_on");
    assert_eq!(a_msg.1, 60, "输出 A 应收到 key=60 的 note_on");

    // ── 替换为输出 B（随后 tick 480, 960 的音符应通过 B 发送）──
    let (output_b, rx_b) = SpyOutput::new();
    manager.set_midi_output(Box::new(output_b));

    // 等待输出 B 收到来自后续音符的 note_on
    // tick 480 的音符 key=64 应该先到达（如果 480 ticks 的墙钟时间已过）
    let b_msg = rx_b
        .recv_timeout(Duration::from_secs(3))
        .expect("替换后，输出 B 应在 3s 内收到 note_on");
    assert!(
        b_msg.1 == 64 || b_msg.1 == 67,
        "输出 B 应收到后续音符的 note_on (key=64 或 67), 实际 key={}",
        b_msg.1
    );

    // 验证输出 A 的 channel 在替换后不再有新事件
    let a_result = rx_a.recv_timeout(Duration::from_millis(300));
    assert!(a_result.is_err(), "旧输出 A 在替换后不应再收到事件");

    manager.stop();
}

/// 场景2：clear_midi_output 后设置新输出 → 新输出接收事件
///
/// 使用跨不同 tick 的多条音符，确保 clear 后仍有未处理事件通过新输出发送。
#[test]
fn test_manager_clear_then_set_midi_output() {
    let mut manager = PlaybackManager::new(480);
    manager.set_current_track_notes(vec![
        crate::NoteEvent {
            tick: 0.0,
            channel: 0,
            key: 64,
            velocity: 100,
            length: 240.0,
        },
        crate::NoteEvent {
            tick: 480.0,
            channel: 0,
            key: 67,
            velocity: 100,
            length: 240.0,
        },
    ]);

    let (output_a, rx_a) = SpyOutput::new();
    manager.set_midi_output(Box::new(output_a));
    manager.play();
    let _ = rx_a
        .recv_timeout(Duration::from_secs(2))
        .expect("输出 A 应收到事件");

    // 清空输出
    manager.clear_midi_output();

    // 验证旧输出 channel 不再有事件
    thread::sleep(Duration::from_millis(100));
    let a_after_clear = rx_a.recv_timeout(Duration::from_millis(100));
    assert!(a_after_clear.is_err(), "清空后旧输出不应再收到事件");

    // 设置新输出 B（后续 tick 480 的音符应通过 B 发送）
    let (output_b, rx_b) = SpyOutput::new();
    manager.set_midi_output(Box::new(output_b));

    let b_msg = rx_b
        .recv_timeout(Duration::from_secs(3))
        .expect("设置新输出后应在 3s 内收到事件");
    assert_eq!(b_msg.1, 67, "输出 B 应收到 key=67 的后续音符 note_on");

    manager.stop();
}

/// 场景3：多次 swap 输出，验证只有最后一个输出活跃
#[test]
fn test_manager_multiple_swaps() {
    let mut manager = PlaybackManager::new(480);
    manager.set_current_track_notes(vec![crate::NoteEvent {
        tick: 0.0,
        channel: 0,
        key: 67,
        velocity: 100,
        length: 960.0,
    }]);

    // 3 个输出依次替换
    let (_out1, rx1) = SpyOutput::new();
    let (_out2, rx2) = SpyOutput::new();
    let (_out3, rx3) = SpyOutput::new();

    manager.set_midi_output(Box::new(_out1));
    manager.set_midi_output(Box::new(_out2));
    manager.set_midi_output(Box::new(_out3));
    manager.play();

    // 只有输出 3 应收到事件
    let msg3 = rx3
        .recv_timeout(Duration::from_secs(2))
        .expect("输出 3 应在 2s 内收到 note_on");
    assert_eq!(msg3.1, 67, "输出 3 应收到 key=67");

    // 输出 1、2 不应收到事件
    assert!(
        rx1.recv_timeout(Duration::from_millis(200)).is_err(),
        "输出 1 不应收到事件（被替换后）"
    );
    assert!(
        rx2.recv_timeout(Duration::from_millis(200)).is_err(),
        "输出 2 不应收到事件（被替换后）"
    );

    manager.stop();
}
