//! 零拷贝 MIDI 文件解析器
//!
//! 这个模块提供了基于内存映射的 MIDI 文件解析，
//! 事件数据直接引用内存映射区域，不进行复制。

use crate::error::{MidiloaderError, Result};
use crate::mmap_model::{MmapMidiFile, MmapTrack};
use crate::model::{Division, Format, Header};
use crate::progress::{Progress, ProgressReporter};
use crate::reader::{BinaryReader, MmapReader};

/// 基于内存映射的 MIDI 文件加载器
#[derive(Debug)]
pub struct MmapMidiLoader {
    reporter: Option<ProgressReporter>,
}

impl MmapMidiLoader {
    /// 创建默认加载器
    pub fn new() -> Self {
        Self { reporter: None }
    }

    /// 使用进度报告器创建加载器
    pub fn with_reporter(reporter: ProgressReporter) -> Self {
        Self {
            reporter: Some(reporter),
        }
    }

    /// 加载 MIDI 文件
    pub fn load<'a>(self, reader: &'a MmapReader) -> Result<MmapMidiFile<'a>> {
        let total_bytes = reader.len() as u64;

        if let Some(ref reporter) = self.reporter {
            reporter.started(total_bytes);
        }

        let mut parser = MmapParser::new(reader, self.reporter.clone());
        let midi_file = parser.parse()?;

        if let Some(ref reporter) = self.reporter {
            reporter.completed();
        }

        Ok(midi_file)
    }

    pub fn analyze_streaming<'a, F>(self, reader: &'a MmapReader, mut on_track: F) -> Result<Header>
    where
        F: FnMut(MmapTrack<'a>, u16) -> Result<()>,
    {
        let total_bytes = reader.len() as u64;

        if let Some(ref reporter) = self.reporter {
            reporter.started(total_bytes);
        }

        let mut parser = MmapParser::new(reader, self.reporter.clone());
        let header = parser.parse_streaming(&mut on_track)?;

        if let Some(ref reporter) = self.reporter {
            reporter.completed();
        }

        Ok(header)
    }
}

impl Default for MmapMidiLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// 基于内存映射的 MIDI 解析器
struct MmapParser<'a> {
    reader: &'a MmapReader,
    reporter: Option<ProgressReporter>,
    position: usize,
}

impl<'a> MmapParser<'a> {
    fn new(reader: &'a MmapReader, reporter: Option<ProgressReporter>) -> Self {
        Self {
            reader,
            reporter,
            position: 0,
        }
    }

    fn parse(&mut self) -> Result<MmapMidiFile<'a>> {
        let header = self.parse_header()?;
        let mut tracks = Vec::with_capacity(header.ntracks as usize);

        for track_index in 0..header.ntracks {
            let track = self.parse_track(track_index, true)?;
            tracks.push(track);

            self.report_progress(header.ntracks, track_index + 1);
        }

        Ok(MmapMidiFile::new(header, tracks))
    }

    fn parse_streaming<F>(&mut self, on_track: &mut F) -> Result<Header>
    where
        F: FnMut(MmapTrack<'a>, u16) -> Result<()>,
    {
        let header = self.parse_header()?;

        for track_index in 0..header.ntracks {
            let track = self.parse_track(track_index, false)?;
            on_track(track, track_index)?;

            self.report_progress(header.ntracks, track_index + 1);
        }

        Ok(header)
    }

    fn report_progress(&self, total_tracks: u16, tracks_parsed: u16) {
        if let Some(ref reporter) = self.reporter {
            reporter.progress(Progress {
                bytes_read: self.position as u64,
                total_bytes: self.reader.len() as u64,
                events_parsed: 0, // 零拷贝模式下不预先计算
                tracks_parsed,
                total_tracks,
            });
        }
    }

    fn parse_header(&mut self) -> Result<Header> {
        const HEADER_CHUNK_TYPE: &[u8] = b"MThd";
        const HEADER_LENGTH: u32 = 6;

        let chunk_type = self.read_bytes(4).ok_or_else(|| self.eof_error(4))?;

        if chunk_type != HEADER_CHUNK_TYPE {
            return Err(MidiloaderError::InvalidHeader(format!(
                "Expected 'MThd', got {:?}",
                chunk_type
            )));
        }

        let length = self.read_u32_be().ok_or_else(|| self.eof_error(4))?;

        if length != HEADER_LENGTH {
            return Err(MidiloaderError::InvalidHeader(format!(
                "Header length must be {}, got {}",
                HEADER_LENGTH, length
            )));
        }

        let format_value = self.read_u16_be().ok_or_else(|| self.eof_error(2))?;
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

        let ntracks = self.read_u16_be().ok_or_else(|| self.eof_error(2))?;

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

    fn parse_division(&mut self) -> Result<Division> {
        const SMPTE_MASK: u16 = 0x8000;
        const FPS_MASK: u16 = 0x7F00;
        const FPS_SHIFT: u16 = 8;
        const TICKS_MASK: u16 = 0x00FF;
        const DROP_FRAME_FPS: i8 = 29;
        const DROP_FRAME_VALUE: i8 = -29;

        let division_raw = self.read_u16_be().ok_or_else(|| self.eof_error(2))?;

        if division_raw & SMPTE_MASK == 0 {
            Ok(Division::TicksPerQuarter(division_raw))
        } else {
            let frames_per_second = ((division_raw & FPS_MASK) >> FPS_SHIFT) as i8;
            let ticks_per_frame = (division_raw & TICKS_MASK) as u8;

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

    fn parse_track(&mut self, track_index: u16, extract_name: bool) -> Result<MmapTrack<'a>> {
        const TRACK_CHUNK_TYPE: &[u8] = b"MTrk";

        let chunk_type = self.read_bytes(4).ok_or_else(|| self.eof_error(4))?;

        if chunk_type != TRACK_CHUNK_TYPE {
            return Err(MidiloaderError::InvalidTrackData(format!(
                "Expected 'MTrk', got {:?}",
                chunk_type
            )));
        }

        let length = self.read_u32_be().ok_or_else(|| self.eof_error(4))? as usize;
        let track_start = self.position;
        let track_end = track_start + length;

        let track_name = if extract_name {
            self.extract_track_name(track_start, track_end)
        } else {
            None
        };

        // 获取轨道数据切片
        let track_data = self
            .reader
            .slice(track_start, track_end)
            .ok_or_else(|| self.eof_error(length))?;

        // 跳过轨道数据
        self.position = track_end;

        if let Some(ref reporter) = self.reporter {
            reporter.track_complete(track_index, 0); // 零拷贝模式下不知道具体事件数
        }

        Ok(MmapTrack::new(track_data, track_name))
    }

    /// 从轨道数据中提取轨道名称
    fn extract_track_name(&self, track_start: usize, track_end: usize) -> Option<&'a str> {
        let data = self.reader.slice(track_start, track_end)?;
        let mut pos = 0;
        let mut running_status: Option<u8> = None;

        while pos < data.len() {
            // 跳过 delta time
            let (_, bytes_read) = read_varlen_at(data, pos)?;
            pos += bytes_read;

            if pos >= data.len() {
                break;
            }

            let status_byte = data[pos];
            let (status, _has_status_byte) = if status_byte & 0x80 == 0x80 {
                pos += 1;
                running_status = Some(status_byte);
                (status_byte, true)
            } else {
                match running_status {
                    Some(s) => (s, false),
                    None => return None, // 格式错误
                }
            };

            // 检查是否是 meta event
            if status == 0xFF {
                if pos >= data.len() {
                    break;
                }

                let meta_type = data[pos];
                pos += 1;

                let (length, bytes_read) = read_varlen_at(data, pos)?;
                pos += bytes_read;

                if meta_type == 0x03 {
                    // Track Name
                    if pos + length as usize <= data.len() {
                        let name_data = &data[pos..pos + length as usize];
                        // 零拷贝尝试：如果不是有效的 UTF-8，则返回 None 或处理
                        return std::str::from_utf8(name_data).ok();
                    }
                }
                pos += length as usize;
            } else if status == 0xF0 || status == 0xF7 {
                // SysEx
                let (length, bytes_read) = read_varlen_at(data, pos)?;
                pos += bytes_read + length as usize;
            } else {
                // 通道事件
                let data_bytes = match status & 0xF0 {
                    0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                    0xC0 | 0xD0 => 1,
                    _ => return None,
                };
                pos += data_bytes;
            }
        }

        None
    }

    fn read_bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let result = self.reader.slice(self.position, self.position + count)?;
        self.position += count;
        Some(result)
    }

    fn read_u16_be(&mut self) -> Option<u16> {
        self.read_bytes(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn read_u32_be(&mut self) -> Option<u32> {
        self.read_bytes(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn eof_error(&self, expected: usize) -> MidiloaderError {
        MidiloaderError::UnexpectedEof {
            position: self.position,
            expected,
            found: self.reader.len().saturating_sub(self.position),
        }
    }
}

/// 从指定位置读取变长数值
fn read_varlen_at(data: &[u8], mut pos: usize) -> Option<(u32, usize)> {
    let start_pos = pos;
    let mut result: u32 = 0;
    let mut count = 0;

    loop {
        if count >= 4 || pos >= data.len() {
            return None;
        }

        let byte = data[pos];
        pos += 1;
        result = (result << 7) | (byte & 0x7F) as u32;

        if byte & 0x80 == 0 {
            break;
        }

        count += 1;
    }

    Some((result, pos - start_pos))
}
