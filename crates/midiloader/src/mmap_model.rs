//! 零拷贝 MIDI 模型
//!
//! 这个模块提供了基于内存映射的 MIDI 数据结构，
//! 事件数据直接引用内存映射区域，不进行复制。

use crate::model::*;
use std::marker::PhantomData;

/// 基于内存映射的 MIDI 文件
#[derive(Debug)]
pub struct MmapMidiFile<'a> {
    pub header: Header,
    pub tracks: Vec<MmapTrack<'a>>,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> MmapMidiFile<'a> {
    pub(crate) fn new(header: Header, tracks: Vec<MmapTrack<'a>>) -> Self {
        Self {
            header,
            tracks,
            _marker: PhantomData,
        }
    }

    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }
}

/// 基于内存映射的 MIDI 轨道
#[derive(Debug, Clone, Copy)]
pub struct MmapTrack<'a> {
    pub name: Option<&'a str>,
    data: &'a [u8],
}

/// 事件引用（完全零拷贝，包含元数据引用）
#[derive(Debug, Clone, PartialEq)]
pub struct MmapEvent<'a> {
    pub delta_time: u32,
    pub kind: MmapEventKind<'a>,
    pub channel: Option<u8>,
    pub raw_data: &'a [u8],
}

/// 零拷贝事件类型
#[derive(Debug, Clone, PartialEq)]
pub enum MmapEventKind<'a> {
    NoteOn(Note),
    NoteOff(Note),
    PolyphonicPressure { key: u8, pressure: u8 },
    CC(CC),
    ProgramChange { program: u8 },
    ChannelPressure { pressure: u8 },
    PitchBend { value: i16 },
    Meta(MmapMetaEvent<'a>),
    SysEx(MmapSysExEvent<'a>),
}

/// 零拷贝元事件
#[derive(Debug, Clone, PartialEq)]
pub enum MmapMetaEvent<'a> {
    SequenceNumber(u16),
    Text(&'a str),
    Copyright(&'a str),
    TrackName(&'a str),
    InstrumentName(&'a str),
    Lyric(&'a str),
    Marker(&'a str),
    CuePoint(&'a str),
    ChannelPrefix(u8),
    EndOfTrack,
    SetTempo(u32),
    SmpteOffset {
        hour: u8,
        minute: u8,
        second: u8,
        frame: u8,
        subframe: u8,
    },
    TimeSignature {
        numerator: u8,
        denominator: u8,
        clocks_per_click: u8,
        notated_32nd_notes_per_beat: u8,
    },
    KeySignature {
        key: i8,
        scale: u8,
    },
    SequencerSpecific(&'a [u8]),
    Unknown {
        meta_type: u8,
        data: &'a [u8],
    },
}

/// 零拷贝系统独占事件
#[derive(Debug, Clone, PartialEq)]
pub enum MmapSysExEvent<'a> {
    Single(&'a [u8]),
    Escape(&'a [u8]),
}

/// 完全零拷贝的事件迭代器
#[derive(Debug)]
pub struct MmapEventIter<'a> {
    data: &'a [u8],
    position: usize,
    running_status: Option<u8>,
}

impl<'a> MmapTrack<'a> {
    pub fn new(data: &'a [u8], name: Option<&'a str>) -> Self {
        Self { name, data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 遍历轨道中的所有事件（原始版本，包含分配）
    pub fn iter_events(&self) -> EventIter<'a> {
        EventIter {
            data: self.data,
            position: 0,
            running_status: None,
        }
    }

    /// 遍历轨道中的所有事件（完全零拷贝版本）
    pub fn iter_mmap_events(&self) -> MmapEventIter<'a> {
        MmapEventIter {
            data: self.data,
            position: 0,
            running_status: None,
        }
    }

    /// 快速遍历关键事件（分析用）
    pub fn iter_fast_events(&self) -> FastEventIter<'a> {
        FastEventIter {
            data: self.data,
            position: 0,
            running_status: None,
        }
    }
}

/// 事件引用
#[derive(Debug, Clone, PartialEq)]
pub struct EventRef<'a> {
    pub delta_time: u32,
    pub kind: EventKind,
    pub channel: Option<u8>,
    pub raw_data: &'a [u8],
}

/// 事件迭代器
#[derive(Debug)]
pub struct EventIter<'a> {
    data: &'a [u8],
    position: usize,
    running_status: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FastEventKind {
    NoteOn { velocity: u8 },
    NoteOff,
    Tempo { tempo: u32 },
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FastEvent {
    pub delta_time: u32,
    pub kind: FastEventKind,
}

#[derive(Debug)]
pub struct FastEventIter<'a> {
    data: &'a [u8],
    position: usize,
    running_status: Option<u8>,
}

impl<'a> Iterator for MmapEventIter<'a> {
    type Item = MmapEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.data.len() {
            return None;
        }

        let start_pos = self.position;

        // 读取 delta time
        let (delta_time, bytes_read) = match read_varlen_at(self.data, self.position) {
            Some(result) => result,
            None => {
                self.position = self.data.len();
                return None;
            }
        };
        self.position += bytes_read;

        // 解析事件
        let (kind, channel, event_size) =
            match parse_mmap_event_at(self.data, self.position, &mut self.running_status) {
                Some(result) => result,
                None => {
                    self.position += 1;
                    return Some(MmapEvent {
                        delta_time,
                        kind: MmapEventKind::Meta(MmapMetaEvent::Unknown {
                            meta_type: 0,
                            data: &[],
                        }),
                        channel: None,
                        raw_data: &self.data[start_pos..self.position.min(self.data.len())],
                    });
                }
            };

        self.position += event_size;
        let raw_data = &self.data[start_pos..self.position.min(self.data.len())];

        Some(MmapEvent {
            delta_time,
            kind,
            channel,
            raw_data,
        })
    }
}

impl<'a> Iterator for EventIter<'a> {
    type Item = EventRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.data.len() {
            return None;
        }

        let start_pos = self.position;

        // 读取 delta time
        let (delta_time, bytes_read) = match read_varlen_at(self.data, self.position) {
            Some(result) => result,
            None => {
                // 无法解析 delta time，跳过剩余数据
                self.position = self.data.len();
                return None;
            }
        };
        self.position += bytes_read;

        // 解析事件
        let (kind, channel, event_size) =
            match parse_event_at(self.data, self.position, &mut self.running_status) {
                Some(result) => result,
                None => {
                    // 无法解析事件，跳过1字节并继续
                    self.position += 1;
                    return Some(EventRef {
                        delta_time,
                        kind: EventKind::Meta(MetaEvent::Unknown {
                            meta_type: 0,
                            data: vec![],
                        }),
                        channel: None,
                        raw_data: &self.data[start_pos..self.position.min(self.data.len())],
                    });
                }
            };

        // 确保 position 前进，防止无限循环
        if event_size == 0 {
            // 如果事件大小为0，至少前进1字节
            self.position += 1;
        } else {
            self.position += event_size;
        }

        let raw_data = &self.data[start_pos..self.position.min(self.data.len())];

        Some(EventRef {
            delta_time,
            kind,
            channel,
            raw_data,
        })
    }
}

impl<'a> Iterator for FastEventIter<'a> {
    type Item = FastEvent;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.data.len() {
            return None;
        }

        let (delta_time, bytes_read) = match read_varlen_at(self.data, self.position) {
            Some(result) => result,
            None => {
                self.position = self.data.len();
                return None;
            }
        };
        self.position += bytes_read;

        let (kind, event_size) =
            match parse_fast_event_at(self.data, self.position, &mut self.running_status) {
                Some(result) => result,
                None => {
                    self.position = self.position.saturating_add(1);
                    return Some(FastEvent {
                        delta_time,
                        kind: FastEventKind::Other,
                    });
                }
            };

        if event_size == 0 {
            self.position = self.position.saturating_add(1);
        } else {
            self.position = self.position.saturating_add(event_size);
        }

        Some(FastEvent { delta_time, kind })
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

/// 解析指定位置的零拷贝事件
fn parse_mmap_event_at<'a>(
    data: &'a [u8],
    mut pos: usize,
    running_status: &mut Option<u8>,
) -> Option<(MmapEventKind<'a>, Option<u8>, usize)> {
    if pos >= data.len() {
        return None;
    }

    let status_byte = data[pos];

    // 检查是否是新的状态字节
    let (status, channel, has_status_byte) = if status_byte & 0x80 == 0x80 {
        pos += 1;
        *running_status = Some(status_byte);

        if is_system_message(status_byte) {
            (status_byte, None, true)
        } else {
            (status_byte & 0xF0, Some(status_byte & 0x0F), true)
        }
    } else {
        // 使用 running status
        match *running_status {
            Some(rs) => {
                if is_system_message(rs) {
                    (rs, None, false)
                } else {
                    (rs & 0xF0, Some(rs & 0x0F), false)
                }
            }
            None => return None,
        }
    };

    // 解析事件数据
    match status {
        0x80 => {
            if pos + 2 > data.len() {
                return None;
            }
            let key = data[pos];
            let _velocity = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            Some((
                MmapEventKind::NoteOff(Note {
                    key,
                    velocity: _velocity,
                }),
                channel,
                size,
            ))
        }
        0x90 => {
            if pos + 2 > data.len() {
                return None;
            }
            let key = data[pos];
            let velocity = data[pos + 1];
            let kind = if velocity == 0 {
                MmapEventKind::NoteOff(Note { key, velocity })
            } else {
                MmapEventKind::NoteOn(Note { key, velocity })
            };
            let size = if has_status_byte { 3 } else { 2 };
            Some((kind, channel, size))
        }
        0xA0 => {
            if pos + 2 > data.len() {
                return None;
            }
            let key = data[pos];
            let pressure = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            Some((
                MmapEventKind::PolyphonicPressure { key, pressure },
                channel,
                size,
            ))
        }
        0xB0 => {
            if pos + 2 > data.len() {
                return None;
            }
            let number = data[pos];
            let value = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            Some((MmapEventKind::CC(CC { number, value }), channel, size))
        }
        0xC0 => {
            if pos + 1 > data.len() {
                return None;
            }
            let program = data[pos];
            let size = if has_status_byte { 2 } else { 1 };
            Some((MmapEventKind::ProgramChange { program }, channel, size))
        }
        0xD0 => {
            if pos + 1 > data.len() {
                return None;
            }
            let pressure = data[pos];
            let size = if has_status_byte { 2 } else { 1 };
            Some((MmapEventKind::ChannelPressure { pressure }, channel, size))
        }
        0xE0 => {
            if pos + 2 > data.len() {
                return None;
            }
            let lsb = data[pos] as i16;
            let msb = data[pos + 1] as i16;
            let value = ((msb << 7) | lsb) - 8192;
            let size = if has_status_byte { 3 } else { 2 };
            Some((MmapEventKind::PitchBend { value }, channel, size))
        }
        0xFF => {
            // Meta event
            if pos >= data.len() {
                return None;
            }
            let meta_type = data[pos];
            pos += 1;

            let (length, bytes_read) = read_varlen_at(data, pos)?;
            pos += bytes_read;

            if pos + length as usize > data.len() {
                return None;
            }

            let meta_data = &data[pos..pos + length as usize];
            let meta_event = parse_mmap_meta_event(meta_type, meta_data)?;

            let total_bytes = (if has_status_byte { 1 } else { 0 }) + 1 + bytes_read + length as usize;
            Some((MmapEventKind::Meta(meta_event), channel, total_bytes))
        }
        0xF0 => {
            // SysEx
            let (length, bytes_read) = read_varlen_at(data, pos)?;
            let total_bytes = (if has_status_byte { 1 } else { 0 }) + bytes_read + length as usize;

            if pos + bytes_read + length as usize > data.len() {
                return None;
            }

            let sysex_data = &data[pos + bytes_read..pos + bytes_read + length as usize];
            Some((
                MmapEventKind::SysEx(MmapSysExEvent::Single(sysex_data)),
                channel,
                total_bytes,
            ))
        }
        0xF7 => {
            // Escape
            let (length, bytes_read) = read_varlen_at(data, pos)?;
            let total_bytes = (if has_status_byte { 1 } else { 0 }) + bytes_read + length as usize;

            if pos + bytes_read + length as usize > data.len() {
                return None;
            }

            let escape_data = &data[pos + bytes_read..pos + bytes_read + length as usize];
            Some((
                MmapEventKind::SysEx(MmapSysExEvent::Escape(escape_data)),
                channel,
                total_bytes,
            ))
        }
        _ => Some((
            MmapEventKind::Meta(MmapMetaEvent::Unknown {
                meta_type: status,
                data: &[],
            }),
            channel,
            1,
        )),
    }
}

/// 解析零拷贝 meta 事件
fn parse_mmap_meta_event<'a>(meta_type: u8, data: &'a [u8]) -> Option<MmapMetaEvent<'a>> {
    match meta_type {
        0x00 => {
            if data.len() >= 2 {
                Some(MmapMetaEvent::SequenceNumber(u16::from_be_bytes([
                    data[0], data[1],
                ])))
            } else {
                None
            }
        }
        0x01 => Some(MmapMetaEvent::Text(std::str::from_utf8(data).unwrap_or(""))),
        0x02 => Some(MmapMetaEvent::Copyright(
            std::str::from_utf8(data).unwrap_or(""),
        )),
        0x03 => Some(MmapMetaEvent::TrackName(
            std::str::from_utf8(data).unwrap_or(""),
        )),
        0x04 => Some(MmapMetaEvent::InstrumentName(
            std::str::from_utf8(data).unwrap_or(""),
        )),
        0x05 => Some(MmapMetaEvent::Lyric(std::str::from_utf8(data).unwrap_or(""))),
        0x06 => Some(MmapMetaEvent::Marker(
            std::str::from_utf8(data).unwrap_or(""),
        )),
        0x07 => Some(MmapMetaEvent::CuePoint(
            std::str::from_utf8(data).unwrap_or(""),
        )),
        0x20 => data.get(0).map(|&ch| MmapMetaEvent::ChannelPrefix(ch)),
        0x2F => Some(MmapMetaEvent::EndOfTrack),
        0x51 => {
            if data.len() >= 3 {
                let tempo = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
                Some(MmapMetaEvent::SetTempo(tempo))
            } else {
                None
            }
        }
        0x54 => {
            if data.len() >= 5 {
                Some(MmapMetaEvent::SmpteOffset {
                    hour: data[0],
                    minute: data[1],
                    second: data[2],
                    frame: data[3],
                    subframe: data[4],
                })
            } else {
                None
            }
        }
        0x58 => {
            if data.len() >= 4 {
                Some(MmapMetaEvent::TimeSignature {
                    numerator: data[0],
                    denominator: data[1],
                    clocks_per_click: data[2],
                    notated_32nd_notes_per_beat: data[3],
                })
            } else {
                None
            }
        }
        0x59 => {
            if data.len() >= 2 {
                Some(MmapMetaEvent::KeySignature {
                    key: data[0] as i8,
                    scale: data[1],
                })
            } else {
                None
            }
        }
        0x7F => Some(MmapMetaEvent::SequencerSpecific(data)),
        _ => Some(MmapMetaEvent::Unknown {
            meta_type,
            data,
        }),
    }
}

/// 解析指定位置的事件
fn parse_event_at(
    data: &[u8],
    mut pos: usize,
    running_status: &mut Option<u8>,
) -> Option<(EventKind, Option<u8>, usize)> {
    if pos >= data.len() {
        return None;
    }

    let status_byte = data[pos];

    // 检查是否是新的状态字节
    let (status, channel, has_status_byte) = if status_byte & 0x80 == 0x80 {
        pos += 1;
        *running_status = Some(status_byte);

        if is_system_message(status_byte) {
            (status_byte, None, true)
        } else {
            (status_byte & 0xF0, Some(status_byte & 0x0F), true)
        }
    } else {
        // 使用 running status
        match *running_status {
            Some(rs) => {
                if is_system_message(rs) {
                    (rs, None, false)
                } else {
                    (rs & 0xF0, Some(rs & 0x0F), false)
                }
            }
            None => return None,
        }
    };

    // 解析事件数据
    match status {
        0x80 => {
            if pos + 2 > data.len() {
                return None;
            }
            let key = data[pos];
            let velocity = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            Some((EventKind::NoteOff(Note { key, velocity }), channel, size))
        }
        0x90 => {
            if pos + 2 > data.len() {
                return None;
            }
            let key = data[pos];
            let velocity = data[pos + 1];
            let kind = if velocity == 0 {
                EventKind::NoteOff(Note { key, velocity })
            } else {
                EventKind::NoteOn(Note { key, velocity })
            };
            let size = if has_status_byte { 3 } else { 2 };
            Some((kind, channel, size))
        }
        0xA0 => {
            if pos + 2 > data.len() {
                return None;
            }
            let key = data[pos];
            let pressure = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            Some((
                EventKind::PolyphonicPressure { key, pressure },
                channel,
                size,
            ))
        }
        0xB0 => {
            if pos + 2 > data.len() {
                return None;
            }
            let number = data[pos];
            let value = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            Some((EventKind::CC(CC { number, value }), channel, size))
        }
        0xC0 => {
            if pos + 1 > data.len() {
                return None;
            }
            let program = data[pos];
            let size = if has_status_byte { 2 } else { 1 };
            Some((EventKind::ProgramChange { program }, channel, size))
        }
        0xD0 => {
            if pos + 1 > data.len() {
                return None;
            }
            let pressure = data[pos];
            let size = if has_status_byte { 2 } else { 1 };
            Some((EventKind::ChannelPressure { pressure }, channel, size))
        }
        0xE0 => {
            if pos + 2 > data.len() {
                return None;
            }
            let lsb = data[pos] as i16;
            let msb = data[pos + 1] as i16;
            let value = ((msb << 7) | lsb) - 8192;
            let size = if has_status_byte { 3 } else { 2 };
            Some((EventKind::PitchBend { value }, channel, size))
        }
        0xFF => {
            // Meta event
            if pos >= data.len() {
                return None;
            }
            let meta_type = data[pos];
            pos += 1;

            let (length, bytes_read) = read_varlen_at(data, pos)?;
            pos += bytes_read;

            if pos + length as usize > data.len() {
                return None;
            }

            let meta_data = &data[pos..pos + length as usize];
            let meta_event = parse_meta_event(meta_type, meta_data)?;

            let total_bytes = 1 + 1 + bytes_read + length as usize;
            Some((EventKind::Meta(meta_event), channel, total_bytes))
        }
        0xF0 => {
            // SysEx
            let (length, bytes_read) = read_varlen_at(data, pos)?;
            let total_bytes = 1 + bytes_read + length as usize;

            if pos + bytes_read + length as usize > data.len() {
                return None;
            }

            let sysex_data = data[pos + bytes_read..pos + bytes_read + length as usize].to_vec();
            Some((
                EventKind::SysEx(SysExEvent::Single(sysex_data)),
                channel,
                total_bytes,
            ))
        }
        0xF7 => {
            // Escape
            let (length, bytes_read) = read_varlen_at(data, pos)?;
            let total_bytes = 1 + bytes_read + length as usize;

            if pos + bytes_read + length as usize > data.len() {
                return None;
            }

            let escape_data = data[pos + bytes_read..pos + bytes_read + length as usize].to_vec();
            Some((
                EventKind::SysEx(SysExEvent::Escape(escape_data)),
                channel,
                total_bytes,
            ))
        }
        _ => {
            // 未知事件类型，跳过1字节防止卡住
            Some((
                EventKind::Meta(MetaEvent::Unknown {
                    meta_type: status,
                    data: vec![],
                }),
                channel,
                1,
            ))
        }
    }
}

fn parse_fast_event_at(
    data: &[u8],
    mut pos: usize,
    running_status: &mut Option<u8>,
) -> Option<(FastEventKind, usize)> {
    if pos >= data.len() {
        return None;
    }

    let status_byte = data[pos];
    let (status, has_status_byte) = if status_byte & 0x80 == 0x80 {
        pos += 1;
        *running_status = Some(status_byte);
        (status_byte, true)
    } else {
        match *running_status {
            Some(rs) => (rs, false),
            None => return None,
        }
    };

    let status = if is_system_message(status) {
        status
    } else {
        status & 0xF0
    };

    match status {
        0x80 => {
            if pos + 2 > data.len() {
                return None;
            }
            let _velocity = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            Some((FastEventKind::NoteOff, size))
        }
        0x90 => {
            if pos + 2 > data.len() {
                return None;
            }
            let velocity = data[pos + 1];
            let size = if has_status_byte { 3 } else { 2 };
            if velocity == 0 {
                Some((FastEventKind::NoteOff, size))
            } else {
                Some((FastEventKind::NoteOn { velocity }, size))
            }
        }
        0xA0 | 0xB0 | 0xE0 => {
            if pos + 2 > data.len() {
                return None;
            }
            let size = if has_status_byte { 3 } else { 2 };
            Some((FastEventKind::Other, size))
        }
        0xC0 | 0xD0 => {
            if pos + 1 > data.len() {
                return None;
            }
            let size = if has_status_byte { 2 } else { 1 };
            Some((FastEventKind::Other, size))
        }
        0xFF => {
            if pos >= data.len() {
                return None;
            }
            let meta_type = data[pos];
            pos += 1;

            let (length, bytes_read) = read_varlen_at(data, pos)?;
            pos += bytes_read;

            if pos + length as usize > data.len() {
                return None;
            }

            let size = (if has_status_byte { 1 } else { 0 }) + 1 + bytes_read + length as usize;
            if meta_type == 0x51 && length >= 3 {
                let tempo = ((data[pos] as u32) << 16)
                    | ((data[pos + 1] as u32) << 8)
                    | (data[pos + 2] as u32);
                Some((FastEventKind::Tempo { tempo }, size))
            } else {
                Some((FastEventKind::Other, size))
            }
        }
        0xF0 | 0xF7 => {
            let (length, bytes_read) = read_varlen_at(data, pos)?;
            let size = (if has_status_byte { 1 } else { 0 }) + bytes_read + length as usize;
            if pos + bytes_read + length as usize > data.len() {
                return None;
            }
            Some((FastEventKind::Other, size))
        }
        _ => Some((FastEventKind::Other, 1)),
    }
}

/// 解析 meta 事件
fn parse_meta_event(meta_type: u8, data: &[u8]) -> Option<MetaEvent> {
    match meta_type {
        0x00 => {
            if data.len() >= 2 {
                Some(MetaEvent::SequenceNumber(u16::from_be_bytes([
                    data[0], data[1],
                ])))
            } else {
                None
            }
        }
        0x01 => Some(MetaEvent::Text(String::from_utf8_lossy(data).into_owned())),
        0x02 => Some(MetaEvent::Copyright(
            String::from_utf8_lossy(data).into_owned(),
        )),
        0x03 => Some(MetaEvent::TrackName(
            String::from_utf8_lossy(data).into_owned(),
        )),
        0x04 => Some(MetaEvent::InstrumentName(
            String::from_utf8_lossy(data).into_owned(),
        )),
        0x05 => Some(MetaEvent::Lyric(String::from_utf8_lossy(data).into_owned())),
        0x06 => Some(MetaEvent::Marker(
            String::from_utf8_lossy(data).into_owned(),
        )),
        0x07 => Some(MetaEvent::CuePoint(
            String::from_utf8_lossy(data).into_owned(),
        )),
        0x20 => data.get(0).map(|&ch| MetaEvent::ChannelPrefix(ch)),
        0x2F => Some(MetaEvent::EndOfTrack),
        0x51 => {
            if data.len() >= 3 {
                let tempo = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
                Some(MetaEvent::SetTempo(tempo))
            } else {
                None
            }
        }
        0x54 => {
            if data.len() >= 5 {
                Some(MetaEvent::SmpteOffset {
                    hour: data[0],
                    minute: data[1],
                    second: data[2],
                    frame: data[3],
                    subframe: data[4],
                })
            } else {
                None
            }
        }
        0x58 => {
            if data.len() >= 4 {
                Some(MetaEvent::TimeSignature {
                    numerator: data[0],
                    denominator: data[1],
                    clocks_per_click: data[2],
                    notated_32nd_notes_per_beat: data[3],
                })
            } else {
                None
            }
        }
        0x59 => {
            if data.len() >= 2 {
                Some(MetaEvent::KeySignature {
                    key: data[0] as i8,
                    scale: data[1],
                })
            } else {
                None
            }
        }
        0x7F => Some(MetaEvent::SequencerSpecific(data.to_vec())),
        _ => Some(MetaEvent::Unknown {
            meta_type,
            data: data.to_vec(),
        }),
    }
}

/// 检查是否为系统消息
fn is_system_message(status: u8) -> bool {
    status == 0xFF || status == 0xF0 || status == 0xF7
}
