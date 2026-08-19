//! 流式解析器单元测试（独立文件，保持 `streaming.rs` < 400 行）
//!
//! 测试数据自举：优先读取本地真实 MIDI；缺失时回退到 [generated_test_midi]
//! 运行时生成的合法 SMF，保证任意环境（CI/新克隆）都能跑通且结果确定。

use midly::TrackEventKind;

use super::player::StreamingMidiPlayer;

const TEST_MIDI_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../test-file/test_note_worker_bench_assets/Erosoul.mid",
);

// ── 测试数据自举 ──────────────────────────────────────────
//
// `test-file/` 目录被 .gitignore 忽略（本地压测资产，不入库），CI 克隆后文件不存在。
// 因此测试优先读取本地真实 MIDI；缺失时回退到 [generated_test_midi] 运行时生成的
// 合法 SMF，保证任意环境（CI/新克隆）都能跑通且结果确定。

/// 加载测试 MIDI：本地真实文件存在则用之，否则用生成的自举数据。
fn load_test_midi() -> Vec<u8> {
    std::fs::read(TEST_MIDI_PATH).unwrap_or_else(|_| generated_test_midi())
}

/// 将事件字节序列封装为 MTrk 块并追加到 `out`。
fn push_track(out: &mut Vec<u8>, events: &[u8]) {
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(events.len() as u32).to_be_bytes());
    out.extend_from_slice(events);
}

/// MIDI 变长数量（VLQ）编码。
fn vlq(mut n: u32) -> Vec<u8> {
    let mut bytes = vec![(n & 0x7F) as u8];
    n >>= 7;
    while n > 0 {
        bytes.push(((n & 0x7F) as u8) | 0x80);
        n >>= 7;
    }
    bytes.reverse();
    bytes
}

/// 生成确定性合法的标准 MIDI 文件（Format 1，3 轨，480 PPQN）：
/// - 轨 0（conductor）：tick 0 处 120 BPM、tick 960 处 90 BPM 两次速度变化
/// - 轨 1：3 个音符（key 60/64/64，ch 0），起音 tick 0/480/960
/// - 轨 2：1 个音符（key 64，ch 1），起音 tick 240（与轨 1 交织）
fn generated_test_midi() -> Vec<u8> {
    let mut out = Vec::new();

    // 头块 MThd：Format 1、3 轨、480 PPQN
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes());
    out.extend_from_slice(&480u16.to_be_bytes());

    // 轨 0：速度变化 + 轨结束
    let mut track0 = vlq(0);
    track0.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 120 BPM
    track0.extend_from_slice(&vlq(960));
    track0.extend_from_slice(&[0xFF, 0x51, 0x03, 0x0A, 0x2C, 0x2A]); // 90 BPM
    track0.extend_from_slice(&vlq(0));
    track0.extend_from_slice(&[0xFF, 0x2F, 0x00]); // End of Track
    push_track(&mut out, &track0);

    // 轨 1：3 个音符（起音 tick 0/480/960）
    let mut track1 = vlq(0);
    track1.extend_from_slice(&[0x90, 60, 100]); // NoteOn ch0 key60 vel100
    track1.extend_from_slice(&vlq(480));
    track1.extend_from_slice(&[0x80, 60, 64]); // NoteOff ch0 key60
    track1.extend_from_slice(&vlq(480));
    track1.extend_from_slice(&[0x90, 64, 100]); // NoteOn ch0 key64
    track1.extend_from_slice(&vlq(480));
    track1.extend_from_slice(&[0x80, 64, 64]); // NoteOff ch0 key64
    track1.extend_from_slice(&vlq(0));
    track1.extend_from_slice(&[0xFF, 0x2F, 0x00]);
    push_track(&mut out, &track1);

    // 轨 2：1 个音符（起音 tick 240，与轨 1 交织）
    let mut track2 = vlq(240);
    track2.extend_from_slice(&[0x91, 64, 80]); // NoteOn ch1 key64 vel80
    track2.extend_from_slice(&vlq(240));
    track2.extend_from_slice(&[0x81, 64, 64]); // NoteOff ch1 key64
    track2.extend_from_slice(&vlq(0));
    track2.extend_from_slice(&[0xFF, 0x2F, 0x00]);
    push_track(&mut out, &track2);

    out
}

/// 验证 `StreamingMidiPlayer` 能正确解析（本地真实文件优先，缺失时回退生成数据）
/// 并逐事件输出。
#[test]
fn test_real_midi_parses() {
    let file_bytes = load_test_midi();
    let mut player =
        StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

    assert!(player.ppqn > 0, "PPQN should be positive");
    assert!(player.total_ticks > 0, "total ticks should be positive");
    assert!(player.track_count() > 0, "track count should be positive");
    assert!(
        !player.tempo_changes.is_empty(),
        "should have tempo changes"
    );

    // 逐事件遍历——验证不 panic
    let mut event_count = 0u64;
    while let Some((tick, _track, _kind)) = player.next_event() {
        event_count += 1;
        // tick 应该非递减（但同一 tick 可有多个事件）
        assert!(
            tick <= player.total_ticks,
            "tick {} should not exceed {}",
            tick,
            player.total_ticks
        );
    }
    assert!(event_count > 0, "should have at least one event");
    assert!(player.is_exhausted(), "player should be exhausted");
}

/// 验证 `next_event()` 返回的事件 tick 按非递减顺序排列。
#[test]
fn test_events_in_order() {
    let file_bytes = load_test_midi();
    let mut player =
        StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

    let mut prev_tick: u64 = 0;
    while let Some((tick, _track, _kind)) = player.next_event() {
        assert!(
            tick >= prev_tick,
            "event tick {} should be >= previous tick {}",
            tick,
            prev_tick
        );
        prev_tick = tick;
    }
}

/// 验证多轨事件互锁——events 来自至少 2 个不同的音轨。
///
/// 这是对逐轨串行 bug 的回归测试：如果 `next_event()` 只输出 track 0 的事件，
/// 则 `distinct_tracks` 集合中只会有一个元素。此测试确保多轨 MIDI 的每一轨
/// 事件都被正确交织输出。
#[test]
fn test_multi_track_interleave() {
    let file_bytes = load_test_midi();
    let mut player =
        StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

    let track_count = player.track_count();
    assert!(
        track_count >= 2,
        "test MIDI should have at least 2 tracks for multi-track test"
    );

    let mut distinct_tracks: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    while let Some((_tick, track_idx, kind)) = player.next_event() {
        // 只统计有意义的 MIDI 事件（忽略 Meta 事件所在的 conductor track）
        if matches!(kind, TrackEventKind::Midi { .. }) {
            distinct_tracks.insert(track_idx);
        }
    }

    assert!(
        distinct_tracks.len() >= 2,
        "MIDI events should come from at least 2 tracks, got {}: {:?}",
        distinct_tracks.len(),
        distinct_tracks,
    );
}

/// 验证 `NoteOn` 事件正确产生 `(key, vel)` 参数。
#[test]
fn test_note_on_events() {
    let file_bytes = load_test_midi();
    let mut player =
        StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

    let mut note_count = 0u64;
    while let Some((_tick, _track, kind)) = player.next_event() {
        if let TrackEventKind::Midi {
            message: midly::MidiMessage::NoteOn { key: _, vel: _ },
            ..
        } = kind
        {
            note_count += 1;
        }
    }
    assert!(note_count > 0, "should have at least one NoteOn event");
}

/// 生成数据的自足性回归测试：不依赖任何外部文件，确保 CI 上生成器产出合法 SMF。
#[test]
fn test_generated_midi_self_sufficient() {
    let bytes = generated_test_midi();
    let mut player = StreamingMidiPlayer::from_bytes(&bytes).expect("生成 MIDI 应可解析");

    assert_eq!(player.ppqn, 480, "PPQN 应为 480");
    assert_eq!(player.track_count(), 3, "应有 3 个轨道");
    assert!(player.total_ticks > 0, "total ticks should be positive");
    assert!(
        !player.tempo_changes.is_empty(),
        "should have tempo changes"
    );

    let mut note_count = 0u64;
    let mut distinct_tracks: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    let mut prev_tick = 0u64;
    while let Some((tick, track_idx, kind)) = player.next_event() {
        assert!(tick >= prev_tick, "tick 应非递减: {tick} < {prev_tick}");
        prev_tick = tick;
        if matches!(kind, TrackEventKind::Midi { .. }) {
            distinct_tracks.insert(track_idx);
        }
        if let TrackEventKind::Midi {
            message: midly::MidiMessage::NoteOn { .. },
            ..
        } = kind
        {
            note_count += 1;
        }
    }
    assert!(player.is_exhausted(), "player should be exhausted");
    assert!(note_count >= 3, "至少应有 3 个 NoteOn，got {note_count}");
    assert!(
        distinct_tracks.len() >= 2,
        "MIDI 事件应来自至少 2 个轨道，got {:?}",
        distinct_tracks,
    );
}
