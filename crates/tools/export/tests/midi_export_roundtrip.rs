//! MIDI 导出循环测试
//!
//! 加载项目测试资源中的真实 MIDI 文件（`test-file/test_unzip_midi/Erosoul.mid`），
//! 从解析结果构造 `MidiExportData`（完全镜像 Runner 的导出来源：
//! `notes / tempo / time_signature / key_signature / PC / CC`），
//! 直接调用 `lumino_export::midi::export_midi_to_bytes` 导出，
//! 再对原始 MIDI 与导出后的 MIDI 做严格比对。
//!
//! 严格比对不仅要求 `midly` 能解析，还要求：
//! 1. 每个音轨都以 `EndOfTrack` 结尾（且其后没有任何事件）——这是其他软件能正确读取的硬约束。
//! 2. 音符、tempo、拍号、调号、PC/CC 在导出后不丢失、不串行。

use std::path::PathBuf;

use lumino_export::midi::{
    MidiControlChangeEvent, MidiExportData, MidiExportOptions, MidiKeySignatureEvent, MidiNoteEvent,
    MidiProgramChangeEvent, MidiTempoEvent, MidiTimeSignatureEvent, MidiTrackData,
    export_midi_to_bytes,
};
use midly::{MetaMessage, MidiMessage, TrackEventKind};

/// 定位仓库根目录下的测试资源 MIDI
fn test_midi_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../test-file/test_unzip_midi/Erosoul.mid")
}

/// 从 `midly` 解析结果构造导出数据（镜像 `editor_midi` 的字段映射：
/// 音符按轨；tempo / 拍号 / 调号聚合到首轨；PC / CC 按轨）。
fn build_export_data_from_smf(smf: &midly::Smf<'_>) -> MidiExportData {
    let mut all_tempos: Vec<(u32, u32)> = Vec::new();
    let mut all_ts: Vec<(u32, u8, u8)> = Vec::new();
    let mut all_ks: Vec<(u32, i8, bool)> = Vec::new();
    let mut pc_by_track: std::collections::HashMap<u16, Vec<MidiProgramChangeEvent>> =
        Default::default();
    let mut cc_by_track: std::collections::HashMap<u16, Vec<MidiControlChangeEvent>> =
        Default::default();

    let mut track_notes: Vec<Vec<MidiNoteEvent>> = Vec::with_capacity(smf.tracks.len());

    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut abs: u32 = 0;
        let mut open: std::collections::HashMap<(u8, u8), (u32, u8)> = Default::default();
        let mut notes: Vec<MidiNoteEvent> = Vec::new();

        for ev in track {
            abs = abs.saturating_add(u32::from(ev.delta));
            match &ev.kind {
                TrackEventKind::Midi { channel, message } => {
                    let ch = u8::from(*channel);
                    match message {
                        MidiMessage::NoteOn { key, vel } => {
                            if u8::from(*vel) != 0 {
                                open.insert((ch, u8::from(*key)), (abs, u8::from(*vel)));
                            }
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            if let Some((start, vel)) = open.remove(&(ch, u8::from(*key))) {
                                notes.push(MidiNoteEvent {
                                    tick: start,
                                    channel: ch,
                                    key: u8::from(*key),
                                    velocity: vel,
                                    duration: abs.saturating_sub(start).max(1),
                                });
                            }
                        }
                        MidiMessage::ProgramChange { program } => {
                            pc_by_track.entry(ti as u16).or_default().push(MidiProgramChangeEvent {
                                tick: abs,
                                channel: ch,
                                program: u8::from(*program),
                            });
                        }
                        MidiMessage::Controller { controller, value } => {
                            cc_by_track.entry(ti as u16).or_default().push(MidiControlChangeEvent {
                                tick: abs,
                                channel: ch,
                                controller: u8::from(*controller),
                                value: u8::from(*value),
                            });
                        }
                        _ => {}
                    }
                }
                TrackEventKind::Meta(meta) => match meta {
                    MetaMessage::Tempo(t) => all_tempos.push((abs, u32::from(*t))),
                    MetaMessage::TimeSignature(num, den, _, _) => {
                        all_ts.push((abs, *num, *den));
                    }
                    MetaMessage::KeySignature(sharps, is_minor) => {
                        all_ks.push((abs, *sharps, *is_minor));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        notes.sort_by_key(|n| (n.tick, n.channel, n.key));
        track_notes.push(notes);
    }

    let mut tracks: Vec<MidiTrackData> = track_notes
        .into_iter()
        .enumerate()
        .map(|(i, notes)| {
            let track_id = i as u16;
            let (program_changes, control_changes) = (
                pc_by_track.get(&track_id).cloned().unwrap_or_default(),
                cc_by_track.get(&track_id).cloned().unwrap_or_default(),
            );
            MidiTrackData {
                notes,
                tempos: if i == 0 {
                    all_tempos
                        .iter()
                        .map(|&(tick, tempo)| MidiTempoEvent { tick, tempo })
                        .collect()
                } else {
                    Vec::new()
                },
                time_signatures: if i == 0 {
                    all_ts
                        .iter()
                        .map(|&(tick, num, den)| MidiTimeSignatureEvent {
                            tick,
                            numerator: num,
                            denominator: den,
                            clocks_per_tick: 24,
                            notated_32nd_notes_per_beat: 8,
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                key_signatures: if i == 0 {
                    all_ks
                        .iter()
                        .map(|&(tick, sharps, is_minor)| MidiKeySignatureEvent {
                            tick,
                            key: sharps,
                            is_major: !is_minor,
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                program_changes,
                control_changes,
                name: None,
            }
        })
        .collect();

    // 保证至少有一条音轨（空 MIDI 也应有 1 条轨道）
    if tracks.is_empty() {
        tracks.push(MidiTrackData::default());
    }

    MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: match smf.header.timing {
                midly::Timing::Metrical(d) => u16::from(d),
                _ => 480,
            },
        },
        tracks,
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
            _ => return Err(format!("音轨 {ti} 最后一个事件不是 EndOfTrack，而是 {last:?}")),
        }
        let eot_count = track
            .iter()
            .filter(|e| matches!(e.kind, TrackEventKind::Meta(MetaMessage::EndOfTrack)))
            .count();
        if eot_count != 1 {
            return Err(format!("音轨 {ti} 包含 {eot_count} 条 EndOfTrack（应为 1）"));
        }
    }
    Ok(())
}

/// 把一条已解析音轨展开为「绝对 tick → 音符摘要」集合，便于与原始 MIDI 比对。
/// 返回 (start_tick, channel, key, velocity, duration)。
fn flatten_notes(track: &[midly::TrackEvent<'_>]) -> Vec<(u32, u8, u8, u8, u32)> {
    let mut abs: u32 = 0;
    let mut out = Vec::new();
    let mut open: std::collections::HashMap<(u8, u8), (u32, u8)> = Default::default();
    for ev in track {
        abs = abs.saturating_add(u32::from(ev.delta));
        if let TrackEventKind::Midi { channel, message } = &ev.kind {
            let ch = u8::from(*channel);
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    if u8::from(*vel) != 0 {
                        open.insert((ch, u8::from(*key)), (abs, u8::from(*vel)));
                    }
                }
                MidiMessage::NoteOff { key, .. } => {
                    if let Some((start, vel)) = open.remove(&(ch, u8::from(*key))) {
                        out.push((start, ch, u8::from(*key), vel, abs.saturating_sub(start)));
                    }
                }
                _ => {}
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    out
}

#[test]
fn test_midi_export_roundtrip_strict() {
    let path = test_midi_path();
    assert!(path.exists(), "测试资源 MIDI 缺失: {:?}", path);

    let original_bytes = std::fs::read(&path).expect("读取原始 MIDI 失败");
    let original = midly::Smf::parse(&original_bytes).expect("原始 MIDI 解析失败");

    let export_data = build_export_data_from_smf(&original);
    let exported = export_midi_to_bytes(&export_data).expect("导出 MIDI 失败");

    // 1) 严格校验：导出文件必须满足其他软件的读取约束（EndOfTrack 在最后）
    if let Err(e) = strict_validate(&exported) {
        panic!(
            "导出 MIDI 严格校验失败（这是导致其他软件无法读取的根因）:\n{e}\n\
             原始音轨数={}，导出音轨数={}",
            original.tracks.len(),
            midly::Smf::parse(&exported).map(|s| s.tracks.len()).unwrap_or(0)
        );
    }

    // 2) 与原始 MIDI 做语义比对：音符集合不应丢失 / 串行
    let exp = midly::Smf::parse(&exported).expect("导出 MIDI 解析失败");
    assert_eq!(original.tracks.len(), exp.tracks.len(), "音轨数量应一致");

    for (ti, (ot, et)) in original.tracks.iter().zip(exp.tracks.iter()).enumerate() {
        let o_notes = flatten_notes(ot);
        let e_notes = flatten_notes(et);
        assert_eq!(
            o_notes, e_notes,
            "音轨 {ti} 导出的音符序列与原文件不一致（丢失或串行）"
        );
    }
}
