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

// ─── 独奏/静音过滤的端到端测试（经默认 MIDI 输出链路） ───
//
// 这些测试验证 `SetTrackPlayStates` 命令在播放线程中真实过滤事件，
// 即"默认调用（xsynth 等）输出"只收到应发声音轨的音符。
// 使用一个计数型 `OutputConnection` 作为默认输出后端，统计到达的 NoteOn 数量。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::OutputConnection;
use crate::playback::PlaybackManager;
use lumino_midi_loader::{MidiDocument, NoteEvent as DocNoteEvent, TrackManager};

/// 计数型 MIDI 输出：统计到达的 NoteOn / NoteOff 数量（用于验证过滤）。
struct CountingOutput {
    counts: Arc<Mutex<(u32, u32)>>,
}

impl OutputConnection for CountingOutput {
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), crate::Error> {
        match data[0] & 0xF0 {
            0x90 => self.counts.lock().expect("计数锁未中毒").0 += 1,
            0x80 => self.counts.lock().expect("计数锁未中毒").1 += 1,
            _ => {}
        }
        Ok(())
    }
    fn close(self: Box<Self>) {}
}

/// 构造两轨文档：track 0 为空（作为当前轨），track 1 含 3 个音符（tick 0/3/6）。
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
fn test_manager_solo_filters_default_output() {
    let counts = Arc::new(Mutex::new((0u32, 0u32)));
    let mut manager = PlaybackManager::new(480);
    manager.set_document(two_track_doc(), 0);
    // 独奏当前空轨（track 0）→ 其他轨（track 1）不应发声
    manager.set_track_play_states(vec![false, false], vec![true, false]);
    manager.set_midi_output(Box::new(CountingOutput {
        counts: Arc::clone(&counts),
    }));
    manager.play();
    std::thread::sleep(Duration::from_millis(100));
    let (note_on, _note_off) = *counts.lock().expect("计数锁未中毒");
    assert_eq!(note_on, 0, "独奏未包含的音轨不应经默认输出发声");
    manager.stop();
}

#[test]
fn test_manager_mute_filters_default_output() {
    let counts = Arc::new(Mutex::new((0u32, 0u32)));
    let mut manager = PlaybackManager::new(480);
    manager.set_document(two_track_doc(), 0);
    // 静音 track 1（无独奏）→ track 1 不应发声
    manager.set_track_play_states(vec![false, true], vec![false, false]);
    manager.set_midi_output(Box::new(CountingOutput {
        counts: Arc::clone(&counts),
    }));
    manager.play();
    std::thread::sleep(Duration::from_millis(100));
    let (note_on, _note_off) = *counts.lock().expect("计数锁未中毒");
    assert_eq!(note_on, 0, "被静音的音轨不应经默认输出发声");
    manager.stop();
}

#[test]
fn test_manager_no_filter_plays_default_output() {
    let counts = Arc::new(Mutex::new((0u32, 0u32)));
    let mut manager = PlaybackManager::new(480);
    manager.set_document(two_track_doc(), 0);
    // 无独奏、无静音 → track 1 应正常发声
    manager.set_track_play_states(vec![false, false], vec![false, false]);
    manager.set_midi_output(Box::new(CountingOutput {
        counts: Arc::clone(&counts),
    }));
    manager.play();
    std::thread::sleep(Duration::from_millis(100));
    let (note_on, _note_off) = *counts.lock().expect("计数锁未中毒");
    assert!(
        note_on > 0,
        "无独奏/静音时默认输出应收到其他轨音符，实际 note_on={}",
        note_on
    );
    manager.stop();
}
