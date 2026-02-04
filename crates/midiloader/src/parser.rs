use std::path::Path;

use crate::error::{MidiloaderError, Result};
use crate::model::*;
use crate::progress::{Progress, ProgressReporter};
use crate::reader::{BinaryReader, MmapReader};

/// MIDI 文件加载选项
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// 是否启用进度报告
    pub report_progress: bool,
    /// 进度报告通道容量
    pub progress_channel_capacity: usize,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            report_progress: false,
            progress_channel_capacity: 1024,
        }
    }
}

impl LoadOptions {
    /// 创建默认选项
    pub fn new() -> Self {
        Self::default()
    }

    /// 启用进度报告
    pub fn with_progress(
        mut self,
    ) -> (
        Self,
        crate::progress::ProgressHandle,
        crate::progress::ProgressReporter,
    ) {
        let (handle, reporter) =
            crate::progress::ProgressHandle::new(self.progress_channel_capacity);
        self.report_progress = true;
        (self, handle, reporter)
    }

    /// 设置进度报告通道容量
    pub fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.progress_channel_capacity = capacity;
        self
    }
}

/// MIDI 文件加载器
///
/// # 示例
///
/// ```rust,no_run
/// use lumino_midiloader::MidiLoader;
///
/// // 简单加载
/// let midi = MidiLoader::new().load("song.mid").unwrap();
///
/// // 带进度报告
/// let (options, handle, reporter) = lumino_midiloader::LoadOptions::new().with_progress();
/// let loader = MidiLoader::with_options_and_reporter(options, reporter);
/// // 在另一个线程中接收进度: handle.recv()
/// ```
#[derive(Debug)]
pub struct MidiLoader {
    _options: LoadOptions,
    reporter: Option<ProgressReporter>,
}

impl MidiLoader {
    /// 创建默认加载器
    pub fn new() -> Self {
        Self::with_options(LoadOptions::default())
    }

    /// 使用指定选项创建加载器
    pub fn with_options(options: LoadOptions) -> Self {
        Self {
            _options: options,
            reporter: None,
        }
    }

    /// 使用指定选项和进度报告器创建加载器
    pub fn with_options_and_reporter(options: LoadOptions, reporter: ProgressReporter) -> Self {
        Self {
            _options: options,
            reporter: Some(reporter),
        }
    }

    /// 加载 MIDI 文件
    ///
    /// # 参数
    ///
    /// * `path` - MIDI 文件路径
    ///
    /// # 返回
    ///
    /// 成功时返回 `MidiFile`，失败时返回 `MidiloaderError`
    pub fn load<P: AsRef<Path>>(self, path: P) -> Result<MidiFile> {
        let reader = MmapReader::open(&path)?;
        let total_bytes = reader.len() as u64;

        if let Some(ref reporter) = self.reporter {
            reporter.started(total_bytes);
        }

        let mut parser = Parser::new(reader, self.reporter.clone());
        let midi_file = parser.parse()?;

        if let Some(ref reporter) = self.reporter {
            reporter.completed();
        }

        Ok(midi_file)
    }
}

impl Default for MidiLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// MIDI 文件解析器
struct Parser {
    reader: MmapReader,
    reporter: Option<ProgressReporter>,
    running_status: Option<u8>,
    total_events: u64,
}

/// 创建 UnexpectedEof 错误的辅助函数
fn eof_error(position: usize, expected: usize, found: usize) -> MidiloaderError {
    MidiloaderError::UnexpectedEof {
        position,
        expected,
        found,
    }
}

impl Parser {
    fn new(reader: MmapReader, reporter: Option<ProgressReporter>) -> Self {
        Self {
            reader,
            reporter,
            running_status: None,
            total_events: 0,
        }
    }

    fn parse(&mut self) -> Result<MidiFile> {
        let header = self.parse_header()?;
        let mut tracks = Vec::with_capacity(header.ntracks as usize);

        for track_index in 0..header.ntracks {
            let track = self.parse_track(track_index)?;
            tracks.push(track);

            self.report_progress(header.ntracks, track_index + 1);
        }

        Ok(MidiFile { header, tracks })
    }

    fn report_progress(&self, total_tracks: u16, tracks_parsed: u16) {
        if let Some(ref reporter) = self.reporter {
            reporter.progress(Progress {
                bytes_read: self.reader.position() as u64,
                total_bytes: self.reader.len() as u64,
                events_parsed: self.total_events,
                tracks_parsed,
                total_tracks,
            });
        }
    }

    fn parse_header(&mut self) -> Result<Header> {
        const HEADER_CHUNK_TYPE: &[u8] = b"MThd";
        const HEADER_LENGTH: u32 = 6;

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let chunk_type = self.reader.read(4).ok_or(eof_error(pos, 4, rem))?;

        if chunk_type != HEADER_CHUNK_TYPE {
            return Err(MidiloaderError::InvalidHeader(format!(
                "Expected 'MThd', got {:?}",
                chunk_type
            )));
        }

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let length = self.reader.read_u32_be().ok_or(eof_error(pos, 4, rem))?;

        if length != HEADER_LENGTH {
            return Err(MidiloaderError::InvalidHeader(format!(
                "Header length must be {}, got {}",
                HEADER_LENGTH, length
            )));
        }

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let format_value = self.reader.read_u16_be().ok_or(eof_error(pos, 2, rem))?;
        let format = match format_value {
            0 => Format::SingleTrack,
            1 => Format::MultiTrackSync,
            2 => Format::MultiTrackIndependent,
            _ => {
                return Err(MidiloaderError::UnsupportedFormat(format!(
                    "Format {} is not supported",
                    format_value
                )));
            }
        };

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let ntracks = self.reader.read_u16_be().ok_or(eof_error(pos, 2, rem))?;

        if format == Format::SingleTrack && ntracks != 1 {
            return Err(MidiloaderError::InvalidHeader(
                "Format 0 must have exactly 1 track".to_string(),
            ));
        }

        let division = self.parse_division()?;

        Ok(Header {
            format,
            ntracks,
            division,
        })
    }

    /// 解析时间分割信息
    ///
    /// MIDI 标准支持两种时间分割格式：
    /// - 基于节拍的（Ticks per Quarter Note）
    /// - 基于 SMPTE 时间的
    fn parse_division(&mut self) -> Result<Division> {
        const SMPTE_MASK: u16 = 0x8000;
        const FPS_MASK: u16 = 0x7F00;
        const FPS_SHIFT: u16 = 8;
        const TICKS_MASK: u16 = 0x00FF;
        const DROP_FRAME_FPS: i8 = 29;
        const DROP_FRAME_VALUE: i8 = -29;

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let division_raw = self.reader.read_u16_be().ok_or(eof_error(pos, 2, rem))?;

        if division_raw & SMPTE_MASK == 0 {
            // 基于节拍的时间分割
            Ok(Division::TicksPerQuarter(division_raw))
        } else {
            // 基于 SMPTE 的时间分割
            let frames_per_second = ((division_raw & FPS_MASK) >> FPS_SHIFT) as i8;
            let ticks_per_frame = (division_raw & TICKS_MASK) as u8;

            // 处理 29.97fps drop-frame 的特殊情况
            let fps = if frames_per_second == DROP_FRAME_FPS {
                DROP_FRAME_VALUE
            } else {
                frames_per_second
            };

            Ok(Division::Smpte {
                frames_per_second: fps,
                ticks_per_frame,
            })
        }
    }

    fn parse_track(&mut self, track_index: u16) -> Result<Track> {
        const TRACK_CHUNK_TYPE: &[u8] = b"MTrk";

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let chunk_type = self.reader.read(4).ok_or(eof_error(pos, 4, rem))?;

        if chunk_type != TRACK_CHUNK_TYPE {
            return Err(MidiloaderError::InvalidTrackData(format!(
                "Expected 'MTrk', got {:?}",
                chunk_type
            )));
        }

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let length = self.reader.read_u32_be().ok_or(eof_error(pos, 4, rem))? as usize;
        let track_start = self.reader.position();
        let track_end = track_start + length;

        let mut events = Vec::new();
        let mut track_name = None;
        self.running_status = None;

        while self.reader.position() < track_end {
            let delta_time = self
                .reader
                .read_varlen()
                .ok_or(MidiloaderError::InvalidVarLen)?;

            let event = self.parse_event(delta_time, track_end)?;

            // 提取轨道名称
            if let EventKind::Meta(MetaEvent::TrackName(ref name)) = event.kind {
                track_name = Some(name.clone());
            }

            events.push(event);
            self.total_events += 1;
        }

        if let Some(ref reporter) = self.reporter {
            reporter.track_complete(track_index, events.len() as u64);
        }

        Ok(Track {
            name: track_name,
            events,
        })
    }

    fn parse_event(&mut self, delta_time: u32, track_end: usize) -> Result<Event> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let status_byte = self.reader.peek(1).ok_or(eof_error(pos, 1, rem))?[0];

        let (status, channel) = if status_byte & 0x80 == 0x80 {
            // 新的状态字节
            self.reader.skip(1);
            self.running_status = Some(status_byte);

            if is_system_message(status_byte) {
                (status_byte, None)
            } else {
                (status_byte & 0xF0, Some(status_byte & 0x0F))
            }
        } else {
            // 使用 running status
            match self.running_status {
                Some(rs) => {
                    if is_system_message(rs) {
                        (rs, None)
                    } else {
                        (rs & 0xF0, Some(rs & 0x0F))
                    }
                }
                None => {
                    return Err(MidiloaderError::InvalidEventData(
                        "Running status used without previous status byte".to_string(),
                    ));
                }
            }
        };

        let kind = self.parse_event_kind(status, track_end)?;

        Ok(Event {
            delta_time,
            kind,
            channel,
        })
    }

    fn parse_event_kind(&mut self, status: u8, track_end: usize) -> Result<EventKind> {
        match status {
            0x80 => self.parse_note_off(),
            0x90 => self.parse_note_on(),
            0xA0 => self.parse_polyphonic_pressure(),
            0xB0 => self.parse_control_change(),
            0xC0 => self.parse_program_change(),
            0xD0 => self.parse_channel_pressure(),
            0xE0 => self.parse_pitch_bend(),
            0xFF => self.parse_meta_event(),
            0xF0 => self.parse_sysex_event(track_end),
            0xF7 => self.parse_escape_event(),
            _ => Err(MidiloaderError::InvalidEventData(format!(
                "Unknown status byte: 0x{:02X}",
                status
            ))),
        }
    }

    fn parse_note_off(&mut self) -> Result<EventKind> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let key = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let velocity = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        Ok(EventKind::NoteOff(Note { key, velocity }))
    }

    fn parse_note_on(&mut self) -> Result<EventKind> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let key = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let velocity = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;

        // 速度为0的 Note On 等同于 Note Off
        if velocity == 0 {
            Ok(EventKind::NoteOff(Note { key, velocity }))
        } else {
            Ok(EventKind::NoteOn(Note { key, velocity }))
        }
    }

    fn parse_polyphonic_pressure(&mut self) -> Result<EventKind> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let key = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let pressure = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        Ok(EventKind::PolyphonicPressure { key, pressure })
    }

    fn parse_control_change(&mut self) -> Result<EventKind> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let number = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let value = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        Ok(EventKind::CC(CC { number, value }))
    }

    fn parse_program_change(&mut self) -> Result<EventKind> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let program = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        Ok(EventKind::ProgramChange { program })
    }

    fn parse_channel_pressure(&mut self) -> Result<EventKind> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let pressure = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        Ok(EventKind::ChannelPressure { pressure })
    }

    fn parse_pitch_bend(&mut self) -> Result<EventKind> {
        const PITCH_BEND_CENTER: i16 = 8192;

        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let lsb = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))? as i16;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let msb = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))? as i16;
        let value = (msb << 7) | lsb;

        Ok(EventKind::PitchBend {
            value: value - PITCH_BEND_CENTER,
        })
    }

    fn parse_meta_event(&mut self) -> Result<EventKind> {
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let meta_type = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
        let length = self
            .reader
            .read_varlen()
            .ok_or(MidiloaderError::InvalidVarLen)? as usize;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let data: Vec<u8> = self
            .reader
            .read(length)
            .ok_or(eof_error(pos, length, rem))?
            .to_vec();

        Ok(EventKind::Meta(
            self.parse_meta_event_kind(meta_type, &data)?,
        ))
    }

    fn parse_meta_event_kind(&self, meta_type: u8, data: &[u8]) -> Result<MetaEvent> {
        match meta_type {
            0x00 => self.parse_sequence_number(data),
            0x01 => Ok(MetaEvent::Text(parse_utf8_text(data))),
            0x02 => Ok(MetaEvent::Copyright(parse_utf8_text(data))),
            0x03 => Ok(MetaEvent::TrackName(parse_utf8_text(data))),
            0x04 => Ok(MetaEvent::InstrumentName(parse_utf8_text(data))),
            0x05 => Ok(MetaEvent::Lyric(parse_utf8_text(data))),
            0x06 => Ok(MetaEvent::Marker(parse_utf8_text(data))),
            0x07 => Ok(MetaEvent::CuePoint(parse_utf8_text(data))),
            0x20 => self.parse_channel_prefix(data),
            0x2F => Ok(MetaEvent::EndOfTrack),
            0x51 => self.parse_set_tempo(data),
            0x54 => self.parse_smpte_offset(data),
            0x58 => self.parse_time_signature(data),
            0x59 => self.parse_key_signature(data),
            0x7F => Ok(MetaEvent::SequencerSpecific(data.to_vec())),
            _ => Ok(MetaEvent::Unknown {
                meta_type,
                data: data.to_vec(),
            }),
        }
    }

    fn parse_sequence_number(&self, data: &[u8]) -> Result<MetaEvent> {
        const EXPECTED_LENGTH: usize = 2;

        if data.len() != EXPECTED_LENGTH {
            return Err(MidiloaderError::InvalidEventData(format!(
                "SequenceNumber meta event must have {} bytes, got {}",
                EXPECTED_LENGTH,
                data.len()
            )));
        }
        Ok(MetaEvent::SequenceNumber(u16::from_be_bytes([
            data[0], data[1],
        ])))
    }

    fn parse_channel_prefix(&self, data: &[u8]) -> Result<MetaEvent> {
        const EXPECTED_LENGTH: usize = 1;

        if data.len() != EXPECTED_LENGTH {
            return Err(MidiloaderError::InvalidEventData(format!(
                "ChannelPrefix meta event must have {} byte, got {}",
                EXPECTED_LENGTH,
                data.len()
            )));
        }
        Ok(MetaEvent::ChannelPrefix(data[0]))
    }

    fn parse_set_tempo(&self, data: &[u8]) -> Result<MetaEvent> {
        const EXPECTED_LENGTH: usize = 3;

        if data.len() != EXPECTED_LENGTH {
            return Err(MidiloaderError::InvalidEventData(format!(
                "SetTempo meta event must have {} bytes, got {}",
                EXPECTED_LENGTH,
                data.len()
            )));
        }
        let tempo = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
        Ok(MetaEvent::SetTempo(tempo))
    }

    fn parse_smpte_offset(&self, data: &[u8]) -> Result<MetaEvent> {
        const EXPECTED_LENGTH: usize = 5;

        if data.len() != EXPECTED_LENGTH {
            return Err(MidiloaderError::InvalidEventData(format!(
                "SmpteOffset meta event must have {} bytes, got {}",
                EXPECTED_LENGTH,
                data.len()
            )));
        }
        Ok(MetaEvent::SmpteOffset {
            hour: data[0],
            minute: data[1],
            second: data[2],
            frame: data[3],
            subframe: data[4],
        })
    }

    fn parse_time_signature(&self, data: &[u8]) -> Result<MetaEvent> {
        const EXPECTED_LENGTH: usize = 4;

        if data.len() != EXPECTED_LENGTH {
            return Err(MidiloaderError::InvalidEventData(format!(
                "TimeSignature meta event must have {} bytes, got {}",
                EXPECTED_LENGTH,
                data.len()
            )));
        }
        Ok(MetaEvent::TimeSignature {
            numerator: data[0],
            denominator: data[1],
            clocks_per_click: data[2],
            notated_32nd_notes_per_beat: data[3],
        })
    }

    fn parse_key_signature(&self, data: &[u8]) -> Result<MetaEvent> {
        const EXPECTED_LENGTH: usize = 2;

        if data.len() != EXPECTED_LENGTH {
            return Err(MidiloaderError::InvalidEventData(format!(
                "KeySignature meta event must have {} bytes, got {}",
                EXPECTED_LENGTH,
                data.len()
            )));
        }
        Ok(MetaEvent::KeySignature {
            key: data[0] as i8,
            scale: data[1],
        })
    }

    fn parse_sysex_event(&mut self, track_end: usize) -> Result<EventKind> {
        let length = self.read_sysex_length(track_end)?;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let data: Vec<u8> = self
            .reader
            .read(length)
            .ok_or(eof_error(pos, length, rem))?
            .to_vec();
        Ok(EventKind::SysEx(SysExEvent::Single(data)))
    }

    fn parse_escape_event(&mut self) -> Result<EventKind> {
        let length = self
            .reader
            .read_varlen()
            .ok_or(MidiloaderError::InvalidVarLen)? as usize;
        let pos = self.reader.position();
        let rem = self.reader.remaining();
        let data: Vec<u8> = self
            .reader
            .read(length)
            .ok_or(eof_error(pos, length, rem))?
            .to_vec();
        Ok(EventKind::SysEx(SysExEvent::Escape(data)))
    }

    fn read_sysex_length(&mut self, track_end: usize) -> Result<usize> {
        const SYSEX_END_BYTE: u8 = 0xF7;

        let mut length = 0usize;

        while self.reader.position() < track_end {
            let pos = self.reader.position();
            let rem = self.reader.remaining();
            let byte = self.reader.read_u8().ok_or(eof_error(pos, 1, rem))?;
            length += 1;

            if byte == SYSEX_END_BYTE {
                return Ok(length);
            }
        }

        Err(MidiloaderError::InvalidEventData(
            "SysEx event not terminated with 0xF7".to_string(),
        ))
    }
}

/// 检查是否为系统消息（无通道号）
fn is_system_message(status: u8) -> bool {
    const META_EVENT: u8 = 0xFF;
    const SYSEX_START: u8 = 0xF0;
    const SYSEX_END: u8 = 0xF7;

    status == META_EVENT || status == SYSEX_START || status == SYSEX_END
}

/// 解析 UTF-8 文本，处理无效字符
fn parse_utf8_text(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_system_message() {
        assert!(is_system_message(0xFF));
        assert!(is_system_message(0xF0));
        assert!(is_system_message(0xF7));
        assert!(!is_system_message(0x90)); // Note On
        assert!(!is_system_message(0x80)); // Note Off
    }

    #[test]
    fn test_load_options_with_progress() {
        let (options, _handle, _reporter) = LoadOptions::new().with_progress();
        assert!(options.report_progress);

        // 测试进度句柄能正常工作
        // 创建一个独立的报告器来测试
        let (test_handle, test_reporter) = crate::progress::ProgressHandle::new(1024);
        test_reporter.started(100);

        // 应该能接收到 Started 事件
        match test_handle.recv() {
            Ok(crate::progress::ProgressEvent::Started { total_bytes: 100 }) => {
                // 测试通过
            }
            _ => panic!("Expected Started event with total_bytes=100"),
        }
    }
}
