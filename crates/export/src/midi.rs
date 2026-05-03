//! MIDI 文件导出功能

use std::path::Path;

use midly::{Format, Header, MetaMessage, Smf, Timing, Track, TrackEvent, TrackEventKind};

use crate::error::{ExportError, ExportResult};

/// MIDI 导出选项
#[derive(Debug, Clone, Default)]
pub struct MidiExportOptions {
    /// MIDI 格式 (0 = 单轨道, 1 = 多轨道同步)
    pub format: u16,
    /// PPQN (每四分音符脉冲数)
    pub ppqn: u16,
}

/// MIDI 音符事件
#[derive(Debug, Clone)]
pub struct MidiNoteEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 键号 (0-127)
    pub key: u8,
    /// 力度 (0-127)
    pub velocity: u8,
    /// 持续时间 (tick)
    pub duration: u32,
}

/// MIDI 速度事件
#[derive(Debug, Clone)]
pub struct MidiTempoEvent {
    /// Tick 位置
    pub tick: u32,
    /// 速度值 (微秒每拍)
    pub tempo: u32,
}

/// MIDI 程序变更事件
#[derive(Debug, Clone)]
pub struct MidiProgramChangeEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 程序号 (0-127)
    pub program: u8,
}

/// MIDI 控制变更事件
#[derive(Debug, Clone)]
pub struct MidiControlChangeEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 控制器号 (0-127)
    pub controller: u8,
    /// 控制值 (0-127)
    pub value: u8,
}

/// MIDI 拍号事件
#[derive(Debug, Clone)]
pub struct MidiTimeSignatureEvent {
    /// Tick 位置
    pub tick: u32,
    /// 分子
    pub numerator: u8,
    /// 分母 (2 的幂次)
    pub denominator: u8,
    /// 每拍的时钟数
    pub clocks_per_tick: u8,
    /// 32分音符数
    pub notated_32nd_notes_per_beat: u8,
}

/// MIDI 调号事件
#[derive(Debug, Clone)]
pub struct MidiKeySignatureEvent {
    /// Tick 位置
    pub tick: u32,
    /// 调号 (-7 到 7)
    pub key: i8,
    /// 是否为大调
    pub is_major: bool,
}

/// MIDI 音轨
#[derive(Debug, Clone, Default)]
pub struct MidiTrackData {
    /// 音符事件列表
    pub notes: Vec<MidiNoteEvent>,
    /// 速度事件列表 (通常放在第一个轨道)
    pub tempos: Vec<MidiTempoEvent>,
    /// 程序变更事件列表
    pub program_changes: Vec<MidiProgramChangeEvent>,
    /// 控制变更事件列表
    pub control_changes: Vec<MidiControlChangeEvent>,
    /// 拍号事件列表
    pub time_signatures: Vec<MidiTimeSignatureEvent>,
    /// 调号事件列表
    pub key_signatures: Vec<MidiKeySignatureEvent>,
    /// 轨道名称
    pub name: Option<String>,
}

/// MIDI 导出数据
#[derive(Debug, Clone)]
pub struct MidiExportData {
    /// 导出选项
    pub options: MidiExportOptions,
    /// 轨道列表
    pub tracks: Vec<MidiTrackData>,
}

/// 导出为 MIDI 文件
pub fn export_midi<P: AsRef<Path>>(path: P, data: &MidiExportData) -> ExportResult<()> {
    let buffer = export_midi_to_bytes(data)?;
    std::fs::write(path.as_ref(), buffer)?;
    Ok(())
}

/// 导出 MIDI 到字节数组
pub fn export_midi_to_bytes(data: &MidiExportData) -> ExportResult<Vec<u8>> {
    let smf = build_midi_smf(data);

    let mut buffer = Vec::new();
    smf.write(&mut buffer)
        .map_err(|e| ExportError::MidiWrite(e.to_string()))?;

    Ok(buffer)
}

/// 构建 MIDI SMF 结构
fn build_midi_smf(data: &MidiExportData) -> Smf<'static> {
    // Pre-leak track names once at the top level.
    // midly::TrackEvent<'static> 要求 'static 生命周期，
    // 泄漏发生在导出时，泄漏量 = 轨道数 × 名称字符串长度，通常 < 1KB。
    let leaked_names: Vec<Option<&'static [u8]>> = data
        .tracks
        .iter()
        .map(|t| {
            t.name.as_ref().map(|n| {
                Box::leak(n.clone().into_boxed_str().into_boxed_bytes()) as &'static [u8]
            })
        })
        .collect();

    let format = match data.options.format {
        0 => Format::SingleTrack,
        1 => Format::Parallel,
        2 => Format::Sequential,
        _ => Format::Parallel, // 默认使用格式 1
    };

    let timing = Timing::Metrical(data.options.ppqn.into());
    let header = Header::new(format, timing);

    let mut tracks: Vec<Track<'static>> = Vec::new();

    // 对于格式 0，所有事件合并到一个轨道
    if data.options.format == 0 {
        let combined_name = leaked_names.first().and_then(|n| *n);
        let mut combined_track = build_combined_track(data, combined_name);
        combined_track.sort_by_key(|e| e.delta);
        convert_to_delta_times(&mut combined_track);
        tracks.push(combined_track);
    } else {
        // 对于格式 1，每个 MidiTrackData 对应一个 MIDI 轨道
        // 第一个轨道包含全局元事件（速度、拍号等）
        let mut first_track = true;

        for (track_data, &name_bytes) in data.tracks.iter().zip(leaked_names.iter()) {
            let mut track = build_track(track_data, first_track, name_bytes);
            track.sort_by_key(|e| e.delta);
            convert_to_delta_times(&mut track);
            tracks.push(track);
            first_track = false;
        }
    }

    Smf { header, tracks }
}

/// 构建合并轨道（格式 0）
fn build_combined_track(data: &MidiExportData, track_name: Option<&'static [u8]>) -> Track<'static> {
    let mut events: Vec<TrackEvent<'static>> = Vec::new();

    // 轨道名称（使用第一个轨道的名称）
    if let Some(name_bytes) = track_name {
        events.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(name_bytes)),
        });
    }

    // 收集所有轨道的所有事件
    for track_data in &data.tracks {
        collect_track_events(track_data, &mut events, true);
    }

    events
}

/// 构建单个轨道
fn build_track(track_data: &MidiTrackData, include_globals: bool, track_name: Option<&'static [u8]>) -> Track<'static> {
    let mut events: Vec<TrackEvent<'static>> = Vec::new();

    // 轨道名称（使用已在 build_midi_smf 中预泄漏的名称引用）
    if let Some(name_bytes) = track_name {
        events.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(name_bytes)),
        });
    }

    collect_track_events(track_data, &mut events, include_globals);

    // 轨道结束
    events.push(TrackEvent {
        delta: 0.into(),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });

    events
}

/// 收集轨道事件
fn collect_track_events(
    track_data: &MidiTrackData,
    events: &mut Vec<TrackEvent<'static>>,
    include_globals: bool,
) {
    // 音符事件
    for note in &track_data.notes {
        // 音符开启
        events.push(TrackEvent {
            delta: note.tick.into(),
            kind: TrackEventKind::Midi {
                channel: note.channel.into(),
                message: midly::MidiMessage::NoteOn {
                    key: note.key.into(),
                    vel: note.velocity.into(),
                },
            },
        });

        // 音符关闭
        let end_tick = note.tick.saturating_add(note.duration);
        events.push(TrackEvent {
            delta: end_tick.into(),
            kind: TrackEventKind::Midi {
                channel: note.channel.into(),
                message: midly::MidiMessage::NoteOff {
                    key: note.key.into(),
                    vel: 0.into(),
                },
            },
        });
    }

    // 速度事件 (全局事件)
    if include_globals {
        for tempo in &track_data.tempos {
            let tempo_value = midly::num::u24::try_from(tempo.tempo)
                .unwrap_or_else(|| midly::num::u24::new(500000)); // 默认 120 BPM = 500000 微秒/拍
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
                message: midly::MidiMessage::ProgramChange {
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
                message: midly::MidiMessage::Controller {
                    controller: cc.controller.into(),
                    value: cc.value.into(),
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
}

/// 将绝对时间转换为增量时间
fn convert_to_delta_times(events: &mut [TrackEvent<'_>]) {
    if events.is_empty() {
        return;
    }

    // 先排序
    events.sort_by_key(|e| u32::from(e.delta));

    // 转换为增量时间
    let mut last_tick: u32 = 0;
    for event in events.iter_mut() {
        let current_tick = u32::from(event.delta);
        let delta = current_tick.saturating_sub(last_tick);
        event.delta = delta.into();
        last_tick = current_tick;
    }
}

/// 将 BPM 转换为微秒每拍
#[inline]
pub fn bpm_to_tempo(bpm: f64) -> u32 {
    // 60_000_000 / BPM = 微秒/拍
    let tempo = 60_000_000.0 / bpm;
    tempo.round() as u32
}

/// 将微秒每拍转换为 BPM
#[inline]
pub fn tempo_to_bpm(tempo: u32) -> f64 {
    60_000_000.0 / tempo as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_empty_midi() {
        // 空轨道应导出为有效的 MIDI 文件
        let data = MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: 480,
            },
            tracks: vec![],
        };
        let result = export_midi_to_bytes(&data);
        assert!(result.is_ok(), "empty MIDI should export successfully");
        let bytes = result.unwrap();
        // MIDI 文件头: "MThd" + 6 bytes header
        assert!(bytes.len() >= 14, "MIDI header should be at least 14 bytes");
        assert_eq!(&bytes[0..4], b"MThd", "should start with MThd");
    }

    #[test]
    fn test_export_single_note_midi() {
        let note = MidiNoteEvent {
            tick: 0,
            channel: 0,
            key: 60,
            velocity: 100,
            duration: 480,
        };
        let track = MidiTrackData {
            notes: vec![note],
            tempos: vec![],
            program_changes: vec![],
            control_changes: vec![],
            time_signatures: vec![],
            key_signatures: vec![],
            name: Some(String::from("Test Track")),
        };
        let data = MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: 480,
            },
            tracks: vec![track],
        };
        let result = export_midi_to_bytes(&data);
        assert!(result.is_ok(), "single note MIDI should export successfully");
        let bytes = result.unwrap();
        assert_eq!(&bytes[0..4], b"MThd", "should start with MThd");
        // 格式 1 应包含轨道数据
        assert!(bytes.len() > 14, "should contain track data beyond header");
    }

    #[test]
    fn test_export_format0_single_track() {
        let note = MidiNoteEvent {
            tick: 0,
            channel: 0,
            key: 60,
            velocity: 100,
            duration: 480,
        };
        let track = MidiTrackData {
            notes: vec![note],
            tempos: vec![MidiTempoEvent { tick: 0, tempo: 500000 }],
            program_changes: vec![],
            control_changes: vec![],
            time_signatures: vec![],
            key_signatures: vec![],
            name: None,
        };
        let data = MidiExportData {
            options: MidiExportOptions {
                format: 0,
                ppqn: 480,
            },
            tracks: vec![track],
        };
        let result = export_midi_to_bytes(&data);
        assert!(result.is_ok(), "format 0 MIDI should export successfully");
        let bytes = result.unwrap();
        assert_eq!(&bytes[0..4], b"MThd", "should start with MThd");
        // Format 0: 单个轨道
        assert_eq!(bytes[10], 0, "format 0 should have 1 track (high byte)");
        assert_eq!(bytes[11], 1, "format 0 should have 1 track (low byte)");
    }

    #[test]
    fn test_bpm_conversion_roundtrip() {
        let bpm = 120.0;
        let tempo = bpm_to_tempo(bpm);
        let recovered = tempo_to_bpm(tempo);
        assert!((recovered - bpm).abs() < 0.01, "BPM roundtrip should be precise");
    }

    #[test]
    fn test_convert_to_delta_times() {
        use midly::num::u28;
        let mut events = vec![
            TrackEvent { delta: u28::from(100u32), kind: TrackEventKind::Meta(MetaMessage::TrackName(b"foo")) },
            TrackEvent { delta: u28::from(50u32), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) },
        ];
        convert_to_delta_times(&mut events);
        // After sorting: 50 should be first with delta 50, then 100 with delta 50
        assert_eq!(u32::from(events[0].delta), 50, "first event delta should be 50");
        assert_eq!(u32::from(events[1].delta), 50, "second event delta should be 50");
    }

    #[test]
    fn test_convert_to_delta_times_empty() {
        let mut events: Vec<TrackEvent<'_>> = vec![];
        convert_to_delta_times(&mut events);
        assert!(events.is_empty(), "empty events should remain empty");
    }

    #[test]
    fn test_build_smf_with_track_name() {
        let track = MidiTrackData {
            notes: vec![],
            tempos: vec![],
            program_changes: vec![],
            control_changes: vec![],
            time_signatures: vec![],
            key_signatures: vec![],
            name: Some(String::from("Piano")),
        };
        let data = MidiExportData {
            options: MidiExportOptions { format: 1, ppqn: 480 },
            tracks: vec![track],
        };
        let smf = build_midi_smf(&data);
        assert_eq!(smf.tracks.len(), 1, "should have 1 track");
        // 第一个轨道事件应该是 TrackName meta 事件
        if let Some(first_event) = smf.tracks[0].first() {
            match &first_event.kind {
                TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                    assert_eq!(name, b"Piano");
                }
                _ => panic!("first event should be TrackName"),
            }
        } else {
            panic!("track should have events");
        }
    }
}
