use std::fmt;

/// MIDI 文件结构
#[derive(Debug, Clone, PartialEq)]
pub struct MidiFile {
    /// 文件头信息
    pub header: Header,
    /// 轨道列表
    pub tracks: Vec<Track>,
}

impl MidiFile {
    /// 获取轨道数量
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// 获取所有事件的总数
    pub fn total_events(&self) -> usize {
        self.tracks.iter().map(|t| t.events.len()).sum()
    }

    /// 查找指定名称的轨道
    pub fn find_track_by_name(&self, name: &str) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == name).unwrap_or(false))
    }
}

/// MIDI 文件头
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// 文件格式
    pub format: Format,
    /// 轨道数量
    pub ntracks: u16,
    /// 时间分割方式
    pub division: Division,
}

/// MIDI 文件格式类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// 格式 0：单轨道
    SingleTrack,
    /// 格式 1：多轨道同步
    MultiTrackSync,
    /// 格式 2：多轨道独立
    MultiTrackIndependent,
}

impl Format {
    /// 获取格式编号
    pub fn as_u16(&self) -> u16 {
        match self {
            Format::SingleTrack => 0,
            Format::MultiTrackSync => 1,
            Format::MultiTrackIndependent => 2,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::SingleTrack => write!(f, "Format 0 (Single Track)"),
            Format::MultiTrackSync => write!(f, "Format 1 (Multi Track, Synchronous)"),
            Format::MultiTrackIndependent => write!(f, "Format 2 (Multi Track, Independent)"),
        }
    }
}

/// 时间分割方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Division {
    /// 每四分音符的 tick 数
    TicksPerQuarter(u16),
    /// SMPTE 时间格式
    Smpte {
        /// 每秒帧数（负值表示 drop-frame）
        frames_per_second: i8,
        /// 每帧 tick 数
        ticks_per_frame: u8,
    },
}

impl Division {
    /// 计算每秒 tick 数（仅对 TicksPerQuarter 有效）
    ///
    /// 需要提供 tempo（微秒/四分音符）
    pub fn ticks_per_second(&self, tempo: u32) -> Option<f64> {
        match self {
            Division::TicksPerQuarter(ticks) => {
                // tempo 是微秒/四分音符
                // ticks_per_second = ticks / (tempo / 1_000_000)
                Some((*ticks as f64) * 1_000_000.0 / (tempo as f64))
            }
            Division::Smpte {
                frames_per_second,
                ticks_per_frame,
            } => Some((frames_per_second.abs() as f64) * (*ticks_per_frame as f64)),
        }
    }
}

/// MIDI 轨道
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// 轨道名称（从 TrackName meta 事件提取）
    pub name: Option<String>,
    /// 事件列表
    pub events: Vec<Event>,
}

impl Track {
    /// 创建空轨道
    pub fn new() -> Self {
        Self {
            name: None,
            events: Vec::new(),
        }
    }

    /// 添加事件
    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    /// 获取事件数量
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 获取所有 Note On 事件
    pub fn note_on_events(&self) -> impl Iterator<Item = &Event> {
        self.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::NoteOn(_)))
    }

    /// 计算轨道总时长（以 tick 为单位）
    pub fn total_ticks(&self) -> u32 {
        self.events.iter().map(|e| e.delta_time).sum()
    }
}

impl Default for Track {
    fn default() -> Self {
        Self::new()
    }
}

/// MIDI 事件
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// 距离上一个事件的 tick 数
    pub delta_time: u32,
    /// 事件类型
    pub kind: EventKind,
    /// 通道号（系统事件为 None）
    pub channel: Option<u8>,
}

impl Event {
    /// 创建新事件
    pub fn new(delta_time: u32, kind: EventKind, channel: Option<u8>) -> Self {
        Self {
            delta_time,
            kind,
            channel,
        }
    }

    /// 检查是否为音符事件
    pub fn is_note(&self) -> bool {
        matches!(self.kind, EventKind::NoteOn(_) | EventKind::NoteOff(_))
    }

    /// 检查是否为元事件
    pub fn is_meta(&self) -> bool {
        matches!(self.kind, EventKind::Meta(_))
    }
}

/// 事件类型
#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    /// 音符开
    NoteOn(Note),
    /// 音符关
    NoteOff(Note),
    /// 复音压力（Polyphonic Aftertouch）
    PolyphonicPressure { key: u8, pressure: u8 },
    /// 控制改变
    CC(CC),
    /// 程序改变
    ProgramChange { program: u8 },
    /// 通道压力（Channel Aftertouch）
    ChannelPressure { pressure: u8 },
    /// 弯音
    PitchBend { value: i16 },
    /// 元事件
    Meta(MetaEvent),
    /// 系统独占事件
    SysEx(SysExEvent),
}

/// 音符信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note {
    /// 音符号（0-127）
    pub key: u8,
    /// 速度（0-127）
    pub velocity: u8,
}

impl Note {
    /// 创建新音符
    pub fn new(key: u8, velocity: u8) -> Self {
        Self { key, velocity }
    }

    /// 获取音符号对应的频率（基于 A4=440Hz）
    pub fn frequency(&self) -> f64 {
        const A4_FREQ: f64 = 440.0;
        const A4_KEY: i32 = 69;

        A4_FREQ * 2.0_f64.powf((self.key as i32 - A4_KEY) as f64 / 12.0)
    }

    /// 获取音符名称（如 "C4", "F#5"）
    pub fn name(&self) -> String {
        const NOTE_NAMES: &[&str] = &[
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];

        let octave = (self.key / 12) as i8 - 1;
        let note_index = (self.key % 12) as usize;

        format!("{}{}", NOTE_NAMES[note_index], octave)
    }
}

/// 控制改变信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CC {
    /// 控制器编号（0-127）
    pub number: u8,
    /// 控制器值（0-127）
    pub value: u8,
}

impl CC {
    /// 创建新的控制改变
    pub fn new(number: u8, value: u8) -> Self {
        Self { number, value }
    }

    /// 获取控制器名称（如果是标准控制器）
    pub fn name(&self) -> Option<&'static str> {
        match self.number {
            0 => Some("Bank Select"),
            1 => Some("Modulation Wheel"),
            7 => Some("Channel Volume"),
            10 => Some("Pan"),
            11 => Some("Expression"),
            64 => Some("Sustain Pedal"),
            91 => Some("Reverb"),
            93 => Some("Chorus"),
            _ => None,
        }
    }

    /// 检查是否为常用控制器（音量、声像、表情等）
    pub fn is_common(&self) -> bool {
        matches!(self.number, 0 | 1 | 7 | 10 | 11 | 64 | 91 | 93)
    }
}

/// 元事件类型
#[derive(Debug, Clone, PartialEq)]
pub enum MetaEvent {
    /// 序列号
    SequenceNumber(u16),
    /// 文本
    Text(String),
    /// 版权信息
    Copyright(String),
    /// 轨道名称
    TrackName(String),
    /// 乐器名称
    InstrumentName(String),
    /// 歌词
    Lyric(String),
    /// 标记
    Marker(String),
    /// 提示点
    CuePoint(String),
    /// 通道前缀
    ChannelPrefix(u8),
    /// 轨道结束
    EndOfTrack,
    /// 设置速度（微秒/四分音符）
    SetTempo(u32),
    /// SMPTE 偏移
    SmpteOffset {
        hour: u8,
        minute: u8,
        second: u8,
        frame: u8,
        subframe: u8,
    },
    /// 拍号
    TimeSignature {
        numerator: u8,
        denominator: u8,
        clocks_per_click: u8,
        notated_32nd_notes_per_beat: u8,
    },
    /// 调号
    KeySignature {
        /// 调号（-7 到 7，负数为降号，正数为升号）
        key: i8,
        /// 调式（0 大调，1 小调）
        scale: u8,
    },
    /// 音序器特定信息
    SequencerSpecific(Vec<u8>),
    /// 未知的元事件
    Unknown { meta_type: u8, data: Vec<u8> },
}

impl MetaEvent {
    /// 获取事件类型名称
    pub fn type_name(&self) -> &'static str {
        match self {
            MetaEvent::SequenceNumber(_) => "SequenceNumber",
            MetaEvent::Text(_) => "Text",
            MetaEvent::Copyright(_) => "Copyright",
            MetaEvent::TrackName(_) => "TrackName",
            MetaEvent::InstrumentName(_) => "InstrumentName",
            MetaEvent::Lyric(_) => "Lyric",
            MetaEvent::Marker(_) => "Marker",
            MetaEvent::CuePoint(_) => "CuePoint",
            MetaEvent::ChannelPrefix(_) => "ChannelPrefix",
            MetaEvent::EndOfTrack => "EndOfTrack",
            MetaEvent::SetTempo(_) => "SetTempo",
            MetaEvent::SmpteOffset { .. } => "SmpteOffset",
            MetaEvent::TimeSignature { .. } => "TimeSignature",
            MetaEvent::KeySignature { .. } => "KeySignature",
            MetaEvent::SequencerSpecific(_) => "SequencerSpecific",
            MetaEvent::Unknown { .. } => "Unknown",
        }
    }

    /// 获取速度（BPM）
    pub fn tempo_bpm(&self) -> Option<f64> {
        match self {
            MetaEvent::SetTempo(tempo) => {
                // tempo 是微秒/四分音符
                // BPM = 60_000_000 / tempo
                Some(60_000_000.0 / (*tempo as f64))
            }
            _ => None,
        }
    }
}

/// 系统独占事件
#[derive(Debug, Clone, PartialEq)]
pub enum SysExEvent {
    /// 单个系统独占消息
    Single(Vec<u8>),
    /// 转义序列（用于分割的系统独占消息）
    Escape(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_frequency() {
        // A4 = 440Hz
        let a4 = Note::new(69, 100);
        assert!((a4.frequency() - 440.0).abs() < 0.01);

        // A3 = 220Hz
        let a3 = Note::new(57, 100);
        assert!((a3.frequency() - 220.0).abs() < 0.01);

        // A5 = 880Hz
        let a5 = Note::new(81, 100);
        assert!((a5.frequency() - 880.0).abs() < 0.01);
    }

    #[test]
    fn test_note_name() {
        assert_eq!(Note::new(60, 100).name(), "C4"); // 中央C
        assert_eq!(Note::new(61, 100).name(), "C#4");
        assert_eq!(Note::new(69, 100).name(), "A4"); // A440
        assert_eq!(Note::new(72, 100).name(), "C5");
    }

    #[test]
    fn test_format_display() {
        assert_eq!(Format::SingleTrack.to_string(), "Format 0 (Single Track)");
        assert_eq!(
            Format::MultiTrackSync.to_string(),
            "Format 1 (Multi Track, Synchronous)"
        );
    }

    #[test]
    fn test_format_as_u16() {
        assert_eq!(Format::SingleTrack.as_u16(), 0);
        assert_eq!(Format::MultiTrackSync.as_u16(), 1);
        assert_eq!(Format::MultiTrackIndependent.as_u16(), 2);
    }

    #[test]
    fn test_division_ticks_per_second() {
        // 120 BPM, 480 ticks per quarter
        // tempo = 60_000_000 / 120 = 500_000 微秒/四分音符
        let division = Division::TicksPerQuarter(480);
        let tps = division.ticks_per_second(500_000).unwrap();
        assert!((tps - 960.0).abs() < 0.1); // 480 * 2 = 960 ticks/second

        // SMPTE 30fps, 80 ticks per frame
        let division = Division::Smpte {
            frames_per_second: 30,
            ticks_per_frame: 80,
        };
        let tps = division.ticks_per_second(0).unwrap();
        assert_eq!(tps, 2400.0); // 30 * 80 = 2400 ticks/second
    }

    #[test]
    fn test_meta_event_tempo_bpm() {
        // 120 BPM = 500_000 微秒/四分音符
        let tempo = MetaEvent::SetTempo(500_000);
        assert!((tempo.tempo_bpm().unwrap() - 120.0).abs() < 0.1);

        // 60 BPM = 1_000_000 微秒/四分音符
        let tempo = MetaEvent::SetTempo(1_000_000);
        assert!((tempo.tempo_bpm().unwrap() - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_event_helpers() {
        let note_on = Event::new(0, EventKind::NoteOn(Note::new(60, 100)), Some(0));
        assert!(note_on.is_note());
        assert!(!note_on.is_meta());

        let meta = Event::new(0, EventKind::Meta(MetaEvent::EndOfTrack), None);
        assert!(!meta.is_note());
        assert!(meta.is_meta());
    }

    #[test]
    fn test_track_helpers() {
        let mut track = Track::new();
        assert!(track.is_empty());

        track.push(Event::new(
            480,
            EventKind::NoteOn(Note::new(60, 100)),
            Some(0),
        ));
        track.push(Event::new(
            480,
            EventKind::NoteOff(Note::new(60, 0)),
            Some(0),
        ));

        assert_eq!(track.len(), 2);
        assert_eq!(track.total_ticks(), 960);
        assert_eq!(track.note_on_events().count(), 1);
    }

    #[test]
    fn test_midi_file_helpers() {
        let midi = MidiFile {
            header: Header {
                format: Format::MultiTrackSync,
                ntracks: 2,
                division: Division::TicksPerQuarter(480),
            },
            tracks: vec![
                Track {
                    name: Some("Track 1".to_string()),
                    events: vec![Event::new(0, EventKind::Meta(MetaEvent::EndOfTrack), None)],
                },
                Track {
                    name: Some("Track 2".to_string()),
                    events: vec![
                        Event::new(0, EventKind::NoteOn(Note::new(60, 100)), Some(0)),
                        Event::new(480, EventKind::NoteOff(Note::new(60, 0)), Some(0)),
                        Event::new(0, EventKind::Meta(MetaEvent::EndOfTrack), None),
                    ],
                },
            ],
        };

        assert_eq!(midi.track_count(), 2);
        assert_eq!(midi.total_events(), 4);
        assert!(midi.find_track_by_name("Track 1").is_some());
        assert!(midi.find_track_by_name("Nonexistent").is_none());
    }
}
