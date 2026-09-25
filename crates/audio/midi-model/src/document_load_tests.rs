//! MidiDocument 加载链路验收单测（EXP-006 数据保留三件套）
//!
//! 独立文件引入（document.rs 底部 `#[path] mod load_tests`），保持 document.rs 行数合理。
//! 覆盖：
//! - 同 tick 多 tempo/拍号/调号全保留（`finalize_sorted_events` 不去重）
//! - NoteOff 释放力度透传（`PackedNote` → `NoteEvent`）
//! - 文本类元事件保留（Text/版权/乐器名/CuePoint）

use super::MidiDocument;
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

/// 用 midly 内存构造 SMF 并序列化为字节（不依赖外部测试资产文件）。
fn build_smf_bytes(tracks: Vec<Track<'_>>) -> Vec<u8> {
    let smf = Smf {
        header: Header::new(Format::Parallel, Timing::Metrical(480u16.into())),
        tracks,
    };
    let mut bytes = Vec::new();
    smf.write(&mut bytes).expect("测试 SMF 写入失败");
    bytes
}

type Track<'a> = Vec<TrackEvent<'a>>;

fn tempo_event(tick_abs: u32, micros: u32) -> (u32, TrackEvent<'static>) {
    let tempo = midly::num::u24::try_from(micros).expect("测试 tempo 超出 u24 范围");
    (
        tick_abs,
        TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(tempo)),
        },
    )
}

/// 把绝对 tick 序列转为 delta 编码的事件向量。
fn to_delta_track(abs_events: Vec<(u32, TrackEvent<'static>)>) -> Track<'static> {
    let mut last = 0u32;
    let mut out = Vec::with_capacity(abs_events.len() + 1);
    for (tick, mut ev) in abs_events {
        ev.delta = (tick.saturating_sub(last)).into();
        last = tick;
        out.push(ev);
    }
    out.push(TrackEvent {
        delta: 0.into(),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    out
}

#[test]
fn test_same_tick_multiple_tempos_preserved() {
    // 同 tick 两个 tempo（500000µs=120BPM / 400000µs=150BPM）：历史 dedup 只留一个
    let track = to_delta_track(vec![tempo_event(0, 500_000), tempo_event(0, 400_000)]);
    let bytes = build_smf_bytes(vec![track]);
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("测试 MIDI 解析失败");

    assert_eq!(
        doc.tempo_changes.len(),
        2,
        "同 tick 双 tempo 应全部保留，实际: {:?}",
        doc.tempo_changes
    );
    // 稳定排序：文件序即写入序
    assert!((doc.tempo_changes[0].1 - 120.0).abs() < 0.01);
    assert!((doc.tempo_changes[1].1 - 150.0).abs() < 0.01);
}

#[test]
fn test_same_tick_multiple_time_and_key_signatures_preserved() {
    let track = to_delta_track(vec![
        (
            0,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::TimeSignature(4, 2, 24, 8)),
            },
        ),
        (
            0,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::TimeSignature(3, 2, 24, 8)),
            },
        ),
        (
            0,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::KeySignature(2, false)),
            },
        ),
        (
            0,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::KeySignature(-3, true)),
            },
        ),
    ]);
    let bytes = build_smf_bytes(vec![track]);
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("测试 MIDI 解析失败");

    assert_eq!(doc.time_signatures.len(), 2, "同 tick 双拍号应全部保留");
    assert_eq!(doc.key_signatures.len(), 2, "同 tick 双调号应全部保留");
    assert_eq!(doc.time_signatures[0].1, 4);
    assert_eq!(doc.time_signatures[1].1, 3);
}

#[test]
fn test_note_off_release_velocity_preserved() {
    // NoteOn(vel=100) + 0x80 NoteOff(vel=64)：释放力度必须进文档
    let track = to_delta_track(vec![
        (
            0,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Midi {
                    channel: 0u8.into(),
                    message: MidiMessage::NoteOn {
                        key: 60,
                        vel: 100u8.into(),
                    },
                },
            },
        ),
        (
            480,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Midi {
                    channel: 0u8.into(),
                    message: MidiMessage::NoteOff {
                        key: 60,
                        vel: 64u8.into(),
                    },
                },
            },
        ),
    ]);
    let bytes = build_smf_bytes(vec![track]);
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("测试 MIDI 解析失败");

    assert_eq!(doc.notes.len(), 1);
    assert_eq!(doc.notes[0].len(), 1);
    let note = doc.notes[0].iter().next().expect("应有 1 个音符");
    assert_eq!(note.velocity, 100);
    assert_eq!(note.release_velocity, 64, "NoteOff vel=64 应透传为释放力度");
}

#[test]
fn test_text_meta_events_preserved() {
    // Text / Copyright / InstrumentName / CuePoint：历史流式提取静默丢弃
    let track = to_delta_track(vec![
        (
            0,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::Text(b"hello")),
            },
        ),
        (
            120,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::Copyright(b"(c) test")),
            },
        ),
        (
            240,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::InstrumentName(b"Piano")),
            },
        ),
        (
            480,
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::CuePoint(b"cue1")),
            },
        ),
    ]);
    let bytes = build_smf_bytes(vec![track]);
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("测试 MIDI 解析失败");

    assert_eq!(doc.text_events.len(), 4, "4 条文本类事件应全部保留");
    let kinds: Vec<u8> = doc.text_events.iter().map(|e| e.2).collect();
    assert_eq!(kinds, vec![0x01, 0x02, 0x04, 0x07]);
    assert_eq!(doc.text_events[0].3, b"hello".to_vec());
    assert_eq!(doc.text_events[3].3, b"cue1".to_vec());
}
