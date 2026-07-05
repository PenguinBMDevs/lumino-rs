//! MIDI 文件导出功能
//!
//! 该模块已拆分为以下子模块：
//! - `export`: 主导出逻辑（export_midi, export_midi_to_bytes）
//! - `tracks`: 音轨构建（build_midi_smf, 轨道事件收集）
//! - `encoding`: 编码辅助（bpm_to_tempo, tempo_to_bpm 重导出）
//! - `calc`: 计算辅助（预留）

mod calc;
mod encoding;
mod export;
mod tracks;

pub use encoding::{bpm_to_tempo, tempo_to_bpm};
pub use export::{export_midi, export_midi_to_bytes};

// ── 类型定义 ──────────────────────────────────────────────

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

// ── 测试 ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::tracks::convert_to_delta_times;
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
        let bytes = result.expect("导出空MIDI数据失败");
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
        assert!(
            result.is_ok(),
            "single note MIDI should export successfully"
        );
        let bytes = result.expect("导出单音符MIDI数据失败");
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
            tempos: vec![MidiTempoEvent {
                tick: 0,
                tempo: 500000,
            }],
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
        let bytes = result.expect("导出Format 0 MIDI数据失败");
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
        assert!(
            (recovered - bpm).abs() < 0.01,
            "BPM roundtrip should be precise"
        );
    }

    #[test]
    fn test_convert_to_delta_times() {
        use midly::num::u28;
        let mut events = vec![
            TrackEvent {
                delta: u28::from(100u32),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(b"foo")),
            },
            TrackEvent {
                delta: u28::from(50u32),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        convert_to_delta_times(&mut events);
        // After sorting: 50 should be first with delta 50, then 100 with delta 50
        assert_eq!(
            u32::from(events[0].delta),
            50,
            "first event delta should be 50"
        );
        assert_eq!(
            u32::from(events[1].delta),
            50,
            "second event delta should be 50"
        );
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
            options: MidiExportOptions {
                format: 1,
                ppqn: 480,
            },
            tracks: vec![track],
        };
        let bytes = export_midi_to_bytes(&data).expect("export should succeed for valid data");
        // 用 midly 重新解析验证输出有效性
        let smf = midly::Smf::parse(&bytes).expect("should parse exported MIDI");
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
