//! MIDI 音轨构建逻辑

use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Timing, Track, TrackEvent, TrackEventKind,
};

use super::{MidiExportData, MidiTrackData};
use crate::error::{ExportError, ExportResult};

/// 构建 MIDI SMF 结构
pub(crate) fn build_midi_smf<'a>(
    data: &'a MidiExportData,
    name_bytes: &'a [Option<&'a [u8]>],
) -> ExportResult<Smf<'a>> {
    let format = match data.options.format {
        0 => Format::SingleTrack,
        1 => Format::Parallel,
        2 => Format::Sequential,
        _ => Format::Parallel, // 默认使用格式 1
    };

    let timing = Timing::Metrical(data.options.ppqn.into());
    let header = Header::new(format, timing);

    let mut tracks: Vec<Track<'_>> = Vec::new();

    // 对于格式 0，所有事件合并到一个轨道
    if data.options.format == 0 {
        let combined_name = name_bytes.first().and_then(|n| *n);
        let mut combined_track = build_combined_track(data, combined_name)?;
        convert_to_delta_times(&mut combined_track);
        // EndOfTrack 必须在轨道末尾，且其后不得有任何事件（其他软件读取硬约束）
        combined_track.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        tracks.push(combined_track);
    } else {
        // 对于格式 1，每个 MidiTrackData 对应一个 MIDI 轨道
        // 第一个轨道包含全局元事件（速度、拍号等）
        let mut first_track = true;

        for (track_data, &name) in data.tracks.iter().zip(name_bytes.iter()) {
            let mut track = build_track(track_data, first_track, name)?;
            convert_to_delta_times(&mut track);
            // EndOfTrack 必须在轨道末尾，且其后不得有任何事件（其他软件读取硬约束）
            track.push(TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            });
            tracks.push(track);
            first_track = false;
        }
    }

    Ok(Smf { header, tracks })
}

/// 构建合并轨道（格式 0）
fn build_combined_track<'a>(
    data: &'a MidiExportData,
    track_name: Option<&'a [u8]>,
) -> ExportResult<Track<'a>> {
    let mut events: Vec<TrackEvent<'a>> = Vec::new();

    // 轨道名称（使用第一个轨道的名称）
    if let Some(name_bytes) = track_name {
        events.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(name_bytes)),
        });
    }

    // 收集所有轨道的所有事件
    for track_data in &data.tracks {
        // 格式 0 合并轨：各源轨道的非 0 端口同样带回（tick 0）
        if let Some(port) = track_data.midi_port {
            events.push(TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::MidiPort(port.into())),
            });
        }
        collect_track_events(track_data, &mut events, true)?;
    }

    // EndOfTrack 在 build_midi_smf 中统一追加到轨道末尾（排序/转增量后），
    // 避免此处以 delta=0 加入后被「按绝对 tick 排序」顶到轨道前面，
    // 导致后续事件写在 EndOfTrack 之后（非法 MIDI，其他软件无法读取）。
    Ok(events)
}

/// 构建单个轨道
fn build_track<'a>(
    track_data: &'a MidiTrackData,
    include_globals: bool,
    track_name: Option<&'a [u8]>,
) -> ExportResult<Track<'a>> {
    let mut events: Vec<TrackEvent<'a>> = Vec::new();

    // 轨道名称（使用已在 build_midi_smf 中预泄漏的名称引用）
    if let Some(name_bytes) = track_name {
        events.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(name_bytes)),
        });
    }

    // MIDI 端口（FF 21，仅非 0 端口写入；tick 0，meta 优先级天然排前）
    if let Some(port) = track_data.midi_port {
        events.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::MidiPort(port.into())),
        });
    }

    collect_track_events(track_data, &mut events, include_globals)?;

    // EndOfTrack 在 build_midi_smf 中统一追加到轨道末尾（排序/转增量后），
    // 避免此处以 delta=0 加入后被「按绝对 tick 排序」顶到轨道前面，
    // 导致后续事件写在 EndOfTrack 之后（非法 MIDI，其他软件无法读取）。
    Ok(events)
}

/// 收集轨道事件
fn collect_track_events<'a>(
    track_data: &'a MidiTrackData,
    events: &mut Vec<TrackEvent<'a>>,
    include_globals: bool,
) -> ExportResult<()> {
    // 音符事件
    for note in &track_data.notes {
        // 音符开启
        events.push(TrackEvent {
            delta: note.tick.into(),
            kind: TrackEventKind::Midi {
                channel: note.channel.into(),
                message: MidiMessage::NoteOn {
                    key: note.key,
                    vel: note.velocity.into(),
                },
            },
        });

        // 音符关闭（释放力度来自文档，不再硬编码 0）
        let end_tick = note.tick.saturating_add(note.duration);
        events.push(TrackEvent {
            delta: end_tick.into(),
            kind: TrackEventKind::Midi {
                channel: note.channel.into(),
                message: MidiMessage::NoteOff {
                    key: note.key,
                    vel: note.release_velocity.into(),
                },
            },
        });
    }

    // 速度事件 (全局事件)
    if include_globals {
        for tempo in &track_data.tempos {
            let tempo_value = midly::num::u24::try_from(tempo.tempo).ok_or_else(|| {
                ExportError::InvalidData(format!(
                    "tempo {} exceeds u24 range (0~16777215 µs/beat)",
                    tempo.tempo
                ))
            })?;
            events.push(TrackEvent {
                delta: tempo.tick.into(),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(tempo_value)),
            });
        }
    }

    // 程序变更
    for pc in &track_data.program_changes {
        events.push(TrackEvent {
            delta: pc.tick.into(),
            kind: TrackEventKind::Midi {
                channel: pc.channel.into(),
                message: MidiMessage::ProgramChange {
                    program: pc.program.into(),
                },
            },
        });
    }

    // 控制变更
    for cc in &track_data.control_changes {
        events.push(TrackEvent {
            delta: cc.tick.into(),
            kind: TrackEventKind::Midi {
                channel: cc.channel.into(),
                message: MidiMessage::Controller {
                    controller: cc.controller.into(),
                    value: cc.value.into(),
                },
            },
        });
    }

    // 弯音
    for pb in &track_data.pitch_bends {
        events.push(TrackEvent {
            delta: pb.tick.into(),
            kind: TrackEventKind::Midi {
                channel: pb.channel.into(),
                message: MidiMessage::PitchBend {
                    bend: midly::PitchBend(midly::num::u14::new(pb.value)),
                },
            },
        });
    }

    // 通道触后
    for at in &track_data.channel_aftertouch {
        events.push(TrackEvent {
            delta: at.tick.into(),
            kind: TrackEventKind::Midi {
                channel: at.channel.into(),
                message: MidiMessage::ChannelAftertouch {
                    vel: at.velocity.into(),
                },
            },
        });
    }

    // 复音触后
    for at in &track_data.poly_aftertouch {
        events.push(TrackEvent {
            delta: at.tick.into(),
            kind: TrackEventKind::Midi {
                channel: at.channel.into(),
                message: MidiMessage::Aftertouch {
                    key: at.key.into(),
                    vel: at.velocity.into(),
                },
            },
        });
    }

    // 拍号事件 (全局事件)
    if include_globals {
        for ts in &track_data.time_signatures {
            events.push(TrackEvent {
                delta: ts.tick.into(),
                kind: TrackEventKind::Meta(MetaMessage::TimeSignature(
                    ts.numerator,
                    ts.denominator,
                    ts.clocks_per_tick,
                    ts.notated_32nd_notes_per_beat,
                )),
            });
        }
    }

    // 调号事件 (全局事件)
    if include_globals {
        for ks in &track_data.key_signatures {
            events.push(TrackEvent {
                delta: ks.tick.into(),
                kind: TrackEventKind::Meta(MetaMessage::KeySignature(ks.key, ks.is_major)),
            });
        }
    }

    // 歌词事件
    for ly in &track_data.lyrics {
        events.push(TrackEvent {
            delta: ly.tick.into(),
            kind: TrackEventKind::Meta(MetaMessage::Lyric(&ly.bytes)),
        });
    }

    // 标记事件
    for mk in &track_data.markers {
        events.push(TrackEvent {
            delta: mk.tick.into(),
            kind: TrackEventKind::Meta(MetaMessage::Marker(&mk.bytes)),
        });
    }

    // 文本类元事件（按 meta_type 原样回写）
    for tx in &track_data.text_events {
        let kind = match tx.meta_type {
            0x01 => MetaMessage::Text(&tx.bytes),
            0x02 => MetaMessage::Copyright(&tx.bytes),
            0x04 => MetaMessage::InstrumentName(&tx.bytes),
            0x07 => MetaMessage::CuePoint(&tx.bytes),
            0x08 => MetaMessage::ProgramName(&tx.bytes),
            0x09 => MetaMessage::DeviceName(&tx.bytes),
            // 加载侧只收录上述 6 种；未知类型宁可丢弃也不错写
            _ => continue,
        };
        events.push(TrackEvent {
            delta: tx.tick.into(),
            kind: TrackEventKind::Meta(kind),
        });
    }

    // SysEx 事件
    for sx in &track_data.sys_ex {
        events.push(TrackEvent {
            delta: sx.tick.into(),
            kind: TrackEventKind::SysEx(&sx.bytes),
        });
    }

    Ok(())
}

fn event_priority(kind: &TrackEventKind) -> u8 {
    match kind {
        TrackEventKind::Midi { message, .. } => match message {
            MidiMessage::NoteOff { .. } => 1,
            MidiMessage::Controller { .. } => 2,
            MidiMessage::ProgramChange { .. } => 3,
            MidiMessage::PitchBend { .. } => 4,
            MidiMessage::ChannelAftertouch { .. } => 4,
            MidiMessage::Aftertouch { .. } => 4,
            MidiMessage::NoteOn { .. } => 5,
            _ => 6,
        },
        _ => 0, // Meta 在同 tick 最先（不影响音符/CC 优先级）
    }
}

/// 将绝对时间转换为增量时间
pub(crate) fn convert_to_delta_times(events: &mut [TrackEvent<'_>]) {
    if events.is_empty() {
        return;
    }

    // 按 (tick, priority) 稳定排序：同 tick 时 CC(RPN) < PB < NoteOn，确保 RPN 在 PB 前（yinhe 2026-06-27 13:22）
    events.sort_by(|a, b| {
        u32::from(a.delta)
            .cmp(&u32::from(b.delta))
            .then(event_priority(&a.kind).cmp(&event_priority(&b.kind)))
    });

    // 转换为增量时间
    let mut last_tick: u32 = 0;
    for event in events.iter_mut() {
        let current_tick = u32::from(event.delta);
        let delta = current_tick.saturating_sub(last_tick);
        event.delta = delta.into();
        last_tick = current_tick;
    }
}
