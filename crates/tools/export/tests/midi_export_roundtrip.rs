//! MIDI 导出循环测试
//!
//! 加载项目测试资源中的真实 MIDI 文件（`test-file/test_unzip_midi/Erosoul.mid`），
//! 使用项目**自身的加载器** `lumino_midi_loader::MidiDocument::from_notes_file`
//! （即 App 实际使用的加载链路）把文件解析为 `MidiDocument`，再从文档构造
//! `MidiExportData`（完全镜像 Runner 的导出来源：
//! `notes / tempo / time_signature / key_signature / PC / CC`），
//! 直接调用 `lumino_export::midi::export_midi_to_bytes` 导出，
//! 再对原始 MIDI 与导出后的 MIDI 做严格比对。
//!
//! 严格比对不仅要求 `midly` 能解析，还要求：
//! 1. 每个音轨都以 `EndOfTrack` 结尾（且其后没有任何事件）——其他软件读取硬约束。
//! 2. 音符、tempo、拍号、调号、PC/CC 在导出后不丢失、不串行。
//!
//! 该测试同时覆盖了两个历史 BUG：
//! - 加载器未对 midly 流式产出的音符按 `start_tick` 排序（debug_assert 崩溃 + 区间查询错误）；
//! - 导出时 `EndOfTrack` 被排在轨道前面，导致其他软件无法读取导出文件。

use std::path::PathBuf;

use lumino_export::midi::{
    MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTempoEvent, MidiTimeSignatureEvent,
    MidiTrackData, export_midi_to_bytes, extract_passthrough_events, extract_pc_cc_events,
};
use lumino_midi_loader::{MidiDocument, bpm_to_tempo};
use midly::{MetaMessage, MidiMessage, Smf, TrackEventKind};

/// 定位仓库根目录下的测试资源 MIDI
fn test_midi_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test-file/test_unzip_midi/Erosoul.mid")
}

/// 从加载后的 `MidiDocument` 构造导出数据（走生产级提取函数，
/// 与 `editor_midi` 保存路径同一数据源，保证测试测的是真链路）。
fn build_export_data_from_doc(doc: &MidiDocument) -> MidiExportData {
    let (pc_by_track, cc_by_track) = extract_pc_cc_events(doc);
    let pass = extract_passthrough_events(doc);

    let tracks: Vec<MidiTrackData> = (0..doc.track_count())
        .map(|i| {
            let track_id = i as u16;
            let notes: Vec<MidiNoteEvent> = doc.notes[i]
                .iter()
                .map(|n| MidiNoteEvent {
                    tick: n.start_tick,
                    channel: n.channel,
                    key: n.key,
                    velocity: n.velocity,
                    release_velocity: n.release_velocity,
                    // tick=0 与零长度音符按原样写出（与编辑器保存路径一致，不钳制）
                    duration: n.length(),
                })
                .collect();
            MidiTrackData {
                notes,
                tempos: if i == 0 {
                    doc.tempo_changes
                        .iter()
                        .map(|&(tick, bpm)| MidiTempoEvent {
                            tick,
                            tempo: bpm_to_tempo(bpm as f64),
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                time_signatures: if i == 0 {
                    doc.time_signatures
                        .iter()
                        .map(|&(tick, num, den)| MidiTimeSignatureEvent {
                            tick,
                            numerator: num,
                            denominator: human_denom_to_pow2(den),
                            clocks_per_tick: 24,
                            notated_32nd_notes_per_beat: 8,
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                key_signatures: if i == 0 {
                    pass.key_signatures.clone()
                } else {
                    Vec::new()
                },
                program_changes: pc_by_track.get(&track_id).cloned().unwrap_or_default(),
                control_changes: cc_by_track.get(&track_id).cloned().unwrap_or_default(),
                pitch_bends: pass.pitch_bends.get(&track_id).cloned().unwrap_or_default(),
                channel_aftertouch: pass
                    .channel_aftertouch
                    .get(&track_id)
                    .cloned()
                    .unwrap_or_default(),
                poly_aftertouch: pass
                    .poly_aftertouch
                    .get(&track_id)
                    .cloned()
                    .unwrap_or_default(),
                lyrics: pass.lyrics.get(&track_id).cloned().unwrap_or_default(),
                markers: pass.markers.get(&track_id).cloned().unwrap_or_default(),
                text_events: pass.text_events.get(&track_id).cloned().unwrap_or_default(),
                sys_ex: pass.sys_ex.get(&track_id).cloned().unwrap_or_default(),
                midi_port: pass.midi_ports.get(&track_id).copied(),
                name: doc.track_name(i).map(|s| s.to_string()),
            }
        })
        .collect();

    MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: doc.division,
        },
        tracks,
    }
}

fn human_denom_to_pow2(d: u8) -> u8 {
    match d {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        32 => 5,
        64 => 6,
        _ => 2,
    }
}

/// 严格校验：每个音轨必须恰好以一条 EndOfTrack 结尾，且其后不得有任何事件。
fn strict_validate(bytes: &[u8]) -> Result<(), String> {
    let smf = midly::Smf::parse(bytes).map_err(|e| format!("midly 解析失败: {e}"))?;
    for (ti, track) in smf.tracks.iter().enumerate() {
        let last = track
            .last()
            .ok_or_else(|| format!("音轨 {ti} 为空（缺少 EndOfTrack）"))?;
        match last.kind {
            TrackEventKind::Meta(MetaMessage::EndOfTrack) => {}
            _ => {
                return Err(format!(
                    "音轨 {ti} 最后一个事件不是 EndOfTrack，而是 {last:?}"
                ));
            }
        }
        let eot_count = track
            .iter()
            .filter(|e| matches!(e.kind, TrackEventKind::Meta(MetaMessage::EndOfTrack)))
            .count();
        if eot_count != 1 {
            return Err(format!(
                "音轨 {ti} 包含 {eot_count} 条 EndOfTrack（应为 1）"
            ));
        }
    }
    Ok(())
}

#[test]
#[ignore = "需要本地测试资源 test-file/test_unzip_midi/Erosoul.mid（该文件不在仓库内，仅本地可用；本地用 `cargo test -- --ignored` 运行）"]
fn test_midi_export_roundtrip_strict() {
    let path = test_midi_path();
    assert!(path.exists(), "测试资源 MIDI 缺失: {:?}", path);

    // 使用项目自身的加载器加载（覆盖加载器排序修复）
    let doc = MidiDocument::from_notes_file(&path, None)
        .expect("加载测试 MIDI 失败（应已被加载器排序修复）");
    let loaded_notes: usize = doc.notes.iter().map(|t| t.len()).sum();
    assert!(loaded_notes > 0, "加载后应有音符");

    let export_data = build_export_data_from_doc(&doc);
    let exported = export_midi_to_bytes(&export_data).expect("导出 MIDI 失败");

    // 1) 严格校验：导出文件必须满足其他软件的读取约束（EndOfTrack 在最后）
    strict_validate(&exported).expect("导出 MIDI 严格校验失败（这是导致其他软件无法读取的根因）");

    // 2) 无损往返：用项目自身的加载器把导出的字节重新读回，
    //    其音符数据必须与从原文件加载的结果逐音符一致。
    //    （直接对比「原始 midly 解析」会受加载器的同通道同键重叠重触发语义影响，
    //    因此两端都走加载器，保证比较的是项目真实数据模型下的无损性。）
    let (doc2, _, _) =
        MidiDocument::from_notes_bytes(&exported, None).expect("重新加载导出 MIDI 失败");
    assert_eq!(
        doc.track_count, doc2.track_count,
        "往返后音轨数量应与原文件一致"
    );

    let mut total_orig: usize = 0;
    let mut total_round: usize = 0;
    for ti in 0..doc.track_count() {
        let a: Vec<lumino_midi_model::NoteEvent> = doc.notes[ti].iter().copied().collect();
        let b: Vec<lumino_midi_model::NoteEvent> = doc2.notes[ti].iter().copied().collect();
        total_orig += a.len();
        total_round += b.len();
        assert_eq!(
            a, b,
            "音轨 {ti} 往返后音符不一致（丢失 / 串行 / 力度或时值错误）"
        );
    }
    assert_eq!(total_orig, total_round, "往返后总音符数应一致");
}

/// 构造全类型覆盖的测试 MIDI（内存合成，不依赖外部文件）。
///
/// 覆盖：tick=0 音符、零长度音符、非零释放力度、tempo/拍号/调号、PC/CC、
/// 弯音、通道触后、复音触后、歌词、标记、文本(0x01)、SysEx、轨名、MidiPort。
fn build_full_coverage_midi() -> Vec<u8> {
    use midly::num::{u4, u7, u24, u28};
    fn ev<'a>(delta: u32, kind: TrackEventKind<'a>) -> midly::TrackEvent<'a> {
        midly::TrackEvent {
            delta: u28::new(delta),
            kind,
        }
    }
    let midi = |ch: u8, msg: MidiMessage| TrackEventKind::Midi {
        channel: u4::new(ch),
        message: msg,
    };
    let meta = TrackEventKind::Meta;

    let track0 = vec![
        ev(0, meta(MetaMessage::TrackName(b"Conductor"))),
        ev(0, meta(MetaMessage::Tempo(u24::new(500_000)))),
        ev(0, meta(MetaMessage::TimeSignature(4, 2, 24, 8))),
        ev(0, meta(MetaMessage::KeySignature(2, false))),
        ev(0, meta(MetaMessage::Marker(b"Intro"))),
        ev(0, meta(MetaMessage::EndOfTrack)),
    ];
    let track1 = vec![
        ev(0, meta(MetaMessage::TrackName(b"Piano"))),
        ev(0, meta(MetaMessage::MidiPort(u7::new(1)))),
        // 零长度音符：tick=0 处开即关
        ev(
            0,
            midi(
                0,
                MidiMessage::NoteOn {
                    key: 60,
                    vel: u7::new(100),
                },
            ),
        ),
        ev(
            0,
            midi(
                0,
                MidiMessage::NoteOff {
                    key: 60,
                    vel: u7::new(0),
                },
            ),
        ),
        ev(
            0,
            midi(
                0,
                MidiMessage::NoteOn {
                    key: 62,
                    vel: u7::new(90),
                },
            ),
        ),
        ev(
            0,
            midi(
                0,
                MidiMessage::ProgramChange {
                    program: u7::new(5),
                },
            ),
        ),
        ev(
            0,
            midi(
                0,
                MidiMessage::Controller {
                    controller: u7::new(7),
                    value: u7::new(100),
                },
            ),
        ),
        ev(
            120,
            midi(
                0,
                MidiMessage::PitchBend {
                    bend: midly::PitchBend(midly::num::u14::new(9192)),
                },
            ),
        ),
        ev(
            120,
            midi(0, MidiMessage::ChannelAftertouch { vel: u7::new(70) }),
        ),
        ev(
            0,
            midi(
                0,
                MidiMessage::Aftertouch {
                    key: u7::new(62),
                    vel: u7::new(80),
                },
            ),
        ),
        // 非零释放力度
        ev(
            240,
            midi(
                0,
                MidiMessage::NoteOff {
                    key: 62,
                    vel: u7::new(64),
                },
            ),
        ),
        ev(0, meta(MetaMessage::Lyric(b"la"))),
        ev(0, meta(MetaMessage::Text(b"hello"))),
        ev(0, TrackEventKind::SysEx(b"\x01\x02\x03\xF7")),
        ev(0, meta(MetaMessage::EndOfTrack)),
    ];

    let smf = Smf {
        header: midly::Header::new(
            midly::Format::Parallel,
            midly::Timing::Metrical(midly::num::u15::new(480)),
        ),
        tracks: vec![track0, track1],
    };
    let mut bytes = Vec::new();
    smf.write(&mut bytes).expect("合成测试MIDI失败");
    bytes
}

/// 事件归一化：smf 字节 → 每轨绝对 tick 事件多重集（排序后可比）。
fn normalize_events(bytes: &[u8]) -> Vec<Vec<String>> {
    let smf = Smf::parse(bytes).expect("测试MIDI应可解析");
    smf.tracks
        .iter()
        .map(|track| {
            let mut abs = 0u32;
            let mut out = Vec::new();
            for e in track {
                abs = abs.saturating_add(u32::from(e.delta));
                let s = match &e.kind {
                    TrackEventKind::Midi { channel, message } => {
                        let ch = u8::from(*channel);
                        match message {
                            MidiMessage::NoteOn { key, vel } => {
                                format!("{abs} ON ch{ch} k{key} v{}", u8::from(*vel))
                            }
                            MidiMessage::NoteOff { key, vel } => {
                                format!("{abs} OFF ch{ch} k{key} v{}", u8::from(*vel))
                            }
                            MidiMessage::ProgramChange { program } => {
                                format!("{abs} PC ch{ch} p{}", u8::from(*program))
                            }
                            MidiMessage::Controller { controller, value } => {
                                format!(
                                    "{abs} CC ch{ch} c{} v{}",
                                    u8::from(*controller),
                                    u8::from(*value)
                                )
                            }
                            MidiMessage::PitchBend { bend } => {
                                format!("{abs} PB ch{ch} v{}", bend.as_int())
                            }
                            MidiMessage::ChannelAftertouch { vel } => {
                                format!("{abs} CHAT ch{ch} v{}", u8::from(*vel))
                            }
                            MidiMessage::Aftertouch { key, vel } => {
                                format!("{abs} POLY ch{ch} k{} v{}", u8::from(*key), u8::from(*vel))
                            }
                        }
                    }
                    TrackEventKind::Meta(m) => match m {
                        MetaMessage::Tempo(v) => format!("{abs} TEMPO {}", v.as_int()),
                        MetaMessage::TimeSignature(n, d, c, s) => {
                            format!("{abs} TIMESIG {n}/{d}/{c}/{s}")
                        }
                        MetaMessage::KeySignature(k, m) => format!("{abs} KEYSIG {k}/{m}"),
                        MetaMessage::TrackName(n) => format!("{abs} NAME {n:?}"),
                        MetaMessage::Lyric(b) => format!("{abs} LYRIC {b:?}"),
                        MetaMessage::Marker(b) => format!("{abs} MARKER {b:?}"),
                        MetaMessage::Text(b) => format!("{abs} TEXT {b:?}"),
                        MetaMessage::Copyright(b) => format!("{abs} COPYRIGHT {b:?}"),
                        MetaMessage::InstrumentName(b) => format!("{abs} INST {b:?}"),
                        MetaMessage::CuePoint(b) => format!("{abs} CUE {b:?}"),
                        MetaMessage::ProgramName(b) => format!("{abs} PROG {b:?}"),
                        MetaMessage::DeviceName(b) => format!("{abs} DEV {b:?}"),
                        MetaMessage::MidiPort(p) => format!("{abs} PORT {}", u8::from(*p)),
                        MetaMessage::EndOfTrack => format!("{abs} EOT"),
                        _ => format!("{abs} META-OTHER"),
                    },
                    TrackEventKind::SysEx(b) => format!("{abs} SYSEX {b:?}"),
                    TrackEventKind::Escape(b) => format!("{abs} ESC {b:?}"),
                };
                out.push(s);
            }
            out.sort();
            out
        })
        .collect()
}

#[test]
fn test_midi_save_load_full_roundtrip_equivalence() {
    // 加载→不编辑→保存→逐事件对比：保存=无损往返
    let original = build_full_coverage_midi();
    let (doc, _, _) = MidiDocument::from_notes_bytes(&original, None).expect("加载测试MIDI失败");
    assert_eq!(doc.track_count(), 2);

    let export_data = build_export_data_from_doc(&doc);
    let exported = export_midi_to_bytes(&export_data).expect("导出失败");
    strict_validate(&exported).expect("导出MIDI严格校验失败");

    let want = normalize_events(&original);
    let got = normalize_events(&exported);
    assert_eq!(
        want.len(),
        got.len(),
        "往返后音轨数不一致: {want:?} vs {got:?}"
    );
    for (ti, (w, g)) in want.iter().zip(got.iter()).enumerate() {
        assert_eq!(w, g, "音轨 {ti} 往返后事件不等价");
    }
}
