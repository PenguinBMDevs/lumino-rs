//! High-performance memory-mapped MIDI file parsing
//!
//! This module provides zero-copy parsing of MIDI files using memory mapping,
//! significantly reducing memory usage and improving parse speed for large files.

use crate::{
    prelude::*,
    primitive::{Format, Timing, u28, u7},
    event::{TrackEvent, TrackEventKind, MidiMessage, MetaMessage},
    smf::Header,
};

/// A memory-mapped MIDI file parser with zero-copy event iteration.
///
/// This type provides extremely fast parsing with minimal memory overhead
/// by directly accessing the memory-mapped file without copying data.
#[derive(Debug)]
pub struct MmapSmf<'a> {
    header: Header,
    tracks: Vec<MmapTrack<'a>>,
    _marker: core::marker::PhantomData<&'a [u8]>,
}

impl<'a> MmapSmf<'a> {
    /// Parse a MIDI file from a memory-mapped slice.
    ///
    /// This is the fastest way to parse a MIDI file that has already been loaded
    /// into memory or mapped from disk.
    #[inline]
    pub fn parse(data: &'a [u8]) -> crate::Result<Self> {
        let (header, tracks_data) = parse_header_tracks(data)?;
        let tracks = tracks_data
            .into_iter()
            .map(|track_data| MmapTrack::new(track_data))
            .collect();
        
        Ok(Self {
            header,
            tracks,
            _marker: core::marker::PhantomData,
        })
    }

    /// Get the header information.
    #[inline]
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Get all tracks.
    #[inline]
    pub fn tracks(&self) -> &[MmapTrack<'a>] {
        &self.tracks
    }

    /// Get the number of tracks.
    #[inline]
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// Convert to an owned Smf by copying all data.
    #[cfg(feature = "alloc")]
    pub fn to_owned(&self) -> crate::Smf<'static> {
        let mut tracks = Vec::with_capacity(self.tracks.len());
        
        for track in &self.tracks {
            let mut events = Vec::new();
            for ev in track.iter() {
                if let Ok(e) = ev {
                    events.push(e.to_static());
                }
            }
            tracks.push(events);
        }
        
        crate::Smf {
            header: self.header,
            tracks,
        }
    }
}

/// A single track backed by memory-mapped data.
#[derive(Debug, Clone, Copy)]
pub struct MmapTrack<'a> {
    data: &'a [u8],
}

impl<'a> MmapTrack<'a> {
    #[inline]
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Get the raw track data.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.data
    }

    /// Create an iterator over events in this track.
    #[inline]
    pub fn iter(&self) -> MmapEventIter<'a> {
        MmapEventIter::new(self.data)
    }

    /// Count the number of events in this track (fast path).
    pub fn event_count(&self) -> usize {
        self.iter().count()
    }

    /// Find all note-on events (optimized for playback).
    pub fn note_on_events(&self) -> impl Iterator<Item = (u28, u7, u7, u4)> + 'a {
        self.iter().filter_map(|ev| {
            ev.ok().and_then(|e| match e.kind {
                TrackEventKind::Midi { channel, message: MidiMessage::NoteOn { key, vel } } => {
                    Some((e.delta, key, vel, channel))
                }
                _ => None,
            })
        })
    }
}

/// An iterator over events in a memory-mapped track.
///
/// This iterator performs zero-copy parsing, reading events directly from
/// the memory-mapped file.
#[derive(Debug, Clone)]
pub struct MmapEventIter<'a> {
    data: &'a [u8],
    running_status: Option<u8>,
}

impl<'a> MmapEventIter<'a> {
    #[inline]
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            running_status: None,
        }
    }

    /// Get remaining bytes.
    #[inline]
    pub fn remaining(&self) -> &'a [u8] {
        self.data
    }
}

impl<'a> Iterator for MmapEventIter<'a> {
    type Item = crate::Result<TrackEvent<'a>>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            return None;
        }

        // Fast path: read delta time using optimized varlen
        let (delta, delta_bytes) = match read_varlen_fast(self.data) {
            Some(r) => r,
            None => {
                self.data = &[];
                return Some(Err(crate::Error::new(
                    &crate::ErrorKind::Invalid("truncated delta time")
                )));
            }
        };
        self.data = &self.data[delta_bytes..];

        // Parse the event
        match parse_event_fast(&mut self.data, &mut self.running_status) {
            Ok(kind) => Some(Ok(TrackEvent { delta: u28::new(delta), kind })),
            Err(e) => {
                self.data = &[];
                Some(Err(e))
            }
        }
    }
}

/// Fast varlen reading using branchless techniques where possible.
#[inline(always)]
fn read_varlen_fast(data: &[u8]) -> Option<(u32, usize)> {
    if data.is_empty() {
        return None;
    }

    let mut result: u32 = 0;
    let mut i = 0;
    
    // Unroll the loop for common cases (most delta times are 1-2 bytes)
    while i < 4 && i < data.len() {
        let byte = data[i];
        result = (result << 7) | (byte & 0x7F) as u32;
        i += 1;
        
        if byte & 0x80 == 0 {
            return Some((result, i));
        }
    }
    
    // If we get here with i == 4, the varlen is malformed (too long)
    if i == 4 && data.get(3).map_or(false, |b| b & 0x80 != 0) {
        // In non-strict mode, use what we have
        Some((result, 4))
    } else {
        None
    }
}

/// Fast event parsing with minimal allocations.
#[inline(always)]
fn parse_event_fast<'a>(
    data: &mut &'a [u8],
    running_status: &mut Option<u8>,
) -> crate::Result<TrackEventKind<'a>> {
    if data.is_empty() {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("unexpected end of track")
        ));
    }

    // Read status byte
    let status = data[0];
    
    if status < 0x80 {
        // Running status - use previous status byte
        match *running_status {
            Some(rs) => parse_channel_message(rs, data, running_status),
            None => Err(crate::Error::new(
                &crate::ErrorKind::Invalid("running status without previous status")
            )),
        }
    } else {
        // New status byte
        *data = &data[1..];
        
        match status {
            0x80..=0xEF => {
                *running_status = Some(status);
                parse_channel_message(status, data, running_status)
            }
            0xFF => parse_meta_event(data, running_status),
            0xF0 => parse_sysex(data, running_status),
            0xF7 => parse_escape(data, running_status),
            _ => Err(crate::Error::new(
                &crate::ErrorKind::Invalid("invalid status byte")
            )),
        }
    }
}

#[inline(always)]
fn parse_channel_message<'a>(
    status: u8,
    data: &mut &'a [u8],
    _running_status: &mut Option<u8>,
) -> crate::Result<TrackEventKind<'a>> {
    let channel = crate::num::u4::new(status & 0x0F);
    let msg_type = status >> 4;
    
    // Fast path for common message types
    match msg_type {
        0x8 | 0x9 | 0xA | 0xB | 0xE => {
            // 2-byte messages: NoteOff, NoteOn, Aftertouch, Controller, PitchBend
            if data.len() < 2 {
                return Err(crate::Error::new(
                    &crate::ErrorKind::Invalid("truncated channel message")
                ));
            }
            let b1 = u7::new(data[0]);
            let b2 = u7::new(data[1]);
            *data = &data[2..];
            
            let message = match msg_type {
                0x8 => MidiMessage::NoteOff { key: b1, vel: b2 },
                0x9 => MidiMessage::NoteOn { key: b1, vel: b2 },
                0xA => MidiMessage::Aftertouch { key: b1, vel: b2 },
                0xB => MidiMessage::Controller { controller: b1, value: b2 },
                0xE => {
                    let bend = ((b2.as_int() as u16) << 7 | b1.as_int() as u16) as i16 - 8192;
                    MidiMessage::PitchBend { 
                        bend: crate::PitchBend(crate::num::u14::new((bend + 8192) as u16))
                    }
                }
                _ => unreachable!(),
            };
            
            Ok(TrackEventKind::Midi { channel, message })
        }
        0xC | 0xD => {
            // 1-byte messages: ProgramChange, ChannelAftertouch
            if data.is_empty() {
                return Err(crate::Error::new(
                    &crate::ErrorKind::Invalid("truncated channel message")
                ));
            }
            let b1 = u7::new(data[0]);
            *data = &data[1..];
            
            let message = match msg_type {
                0xC => MidiMessage::ProgramChange { program: b1 },
                0xD => MidiMessage::ChannelAftertouch { vel: b1 },
                _ => unreachable!(),
            };
            
            Ok(TrackEventKind::Midi { channel, message })
        }
        _ => Err(crate::Error::new(
            &crate::ErrorKind::Invalid("unknown channel message type")
        )),
    }
}

#[inline(always)]
fn parse_meta_event<'a>(
    data: &mut &'a [u8],
    running_status: &mut Option<u8>,
) -> crate::Result<TrackEventKind<'a>> {
    *running_status = None;
    
    if data.is_empty() {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("truncated meta event")
        ));
    }
    
    let meta_type = data[0];
    *data = &data[1..];
    
    let (length, len_bytes) = read_varlen_fast(data)
        .ok_or_else(|| crate::Error::new(
            &crate::ErrorKind::Invalid("invalid meta event length")
        ))?;
    *data = &data[len_bytes..];
    
    if data.len() < length as usize {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("truncated meta event data")
        ));
    }
    
    let meta_data = &data[..length as usize];
    *data = &data[length as usize..];
    
    let message = parse_meta_message(meta_type, meta_data)?;
    Ok(TrackEventKind::Meta(message))
}

#[inline(always)]
fn parse_meta_message(meta_type: u8, data: &[u8]) -> crate::Result<MetaMessage> {
    Ok(match meta_type {
        0x00 => MetaMessage::TrackNumber(
            if data.len() >= 2 {
                Some(u16::from_be_bytes([data[0], data[1]]))
            } else {
                None
            }
        ),
        0x01 => MetaMessage::Text(data),
        0x02 => MetaMessage::Copyright(data),
        0x03 => MetaMessage::TrackName(data),
        0x04 => MetaMessage::InstrumentName(data),
        0x05 => MetaMessage::Lyric(data),
        0x06 => MetaMessage::Marker(data),
        0x07 => MetaMessage::CuePoint(data),
        0x08 => MetaMessage::ProgramName(data),
        0x09 => MetaMessage::DeviceName(data),
        0x20 if !data.is_empty() => MetaMessage::MidiChannel(u4::new(data[0])),
        0x21 if !data.is_empty() => MetaMessage::MidiPort(u7::new(data[0])),
        0x2F => MetaMessage::EndOfTrack,
        0x51 if data.len() >= 3 => {
            let tempo = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
            MetaMessage::Tempo(crate::num::u24::new(tempo))
        }
        0x54 if data.len() >= 5 => MetaMessage::SmpteOffset(crate::primitive::SmpteTime::new(
            data[0] & 0x1F,
            data[1],
            data[2],
            data[3],
            data[4],
            match (data[0] >> 5) & 0x03 {
                0 => crate::primitive::Fps::Fps24,
                1 => crate::primitive::Fps::Fps25,
                2 => crate::primitive::Fps::Fps29,
                _ => crate::primitive::Fps::Fps30,
            }
        ).unwrap_or_else(|| crate::primitive::SmpteTime::new(0, 0, 0, 0, 0, crate::primitive::Fps::Fps24).unwrap())),
        0x58 if data.len() >= 4 => MetaMessage::TimeSignature(data[0], data[1], data[2], data[3]),
        0x59 if data.len() >= 2 => MetaMessage::KeySignature(data[0] as i8, data[1] != 0),
        0x7F => MetaMessage::SequencerSpecific(data),
        _ => MetaMessage::Unknown(meta_type, data),
    })
}

#[inline(always)]
fn parse_sysex<'a>(
    data: &mut &'a [u8],
    running_status: &mut Option<u8>,
) -> crate::Result<TrackEventKind<'a>> {
    *running_status = None;
    
    let (length, len_bytes) = read_varlen_fast(data)
        .ok_or_else(|| crate::Error::new(
            &crate::ErrorKind::Invalid("invalid sysex length")
        ))?;
    *data = &data[len_bytes..];
    
    if data.len() < length as usize {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("truncated sysex data")
        ));
    }
    
    let sysex_data = &data[..length as usize];
    *data = &data[length as usize..];
    
    Ok(TrackEventKind::SysEx(sysex_data))
}

#[inline(always)]
fn parse_escape<'a>(
    data: &mut &'a [u8],
    running_status: &mut Option<u8>,
) -> crate::Result<TrackEventKind<'a>> {
    *running_status = None;
    
    let (length, len_bytes) = read_varlen_fast(data)
        .ok_or_else(|| crate::Error::new(
            &crate::ErrorKind::Invalid("invalid escape length")
        ))?;
    *data = &data[len_bytes..];
    
    if data.len() < length as usize {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("truncated escape data")
        ));
    }
    
    let escape_data = &data[..length as usize];
    *data = &data[length as usize..];
    
    Ok(TrackEventKind::Escape(escape_data))
}

/// Parse header and return track data slices.
fn parse_header_tracks(data: &[u8]) -> crate::Result<(Header, Vec<&[u8]>)> {
    if data.len() < 14 {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("file too short for MIDI header")
        ));
    }
    
    // Check for RIFF wrapper
    let data = if &data[..4] == b"RIFF" {
        // Skip RIFF header and find MThd
        find_mthd_in_riff(data)?
    } else {
        data
    };
    
    if &data[..4] != b"MThd" {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("not a MIDI file (missing MThd)")
        ));
    }
    
    let header_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if header_len != 6 {
        return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("invalid MIDI header length")
        ));
    }
    
    let format = match u16::from_be_bytes([data[8], data[9]]) {
        0 => Format::SingleTrack,
        1 => Format::Parallel,
        2 => Format::Sequential,
        _ => return Err(crate::Error::new(
            &crate::ErrorKind::Invalid("invalid MIDI format")
        )),
    };
    
    let num_tracks = u16::from_be_bytes([data[10], data[11]]);
    let timing_raw = u16::from_be_bytes([data[12], data[13]]);
    
    let timing = if timing_raw & 0x8000 != 0 {
        // SMPTE timecode
        let fps_raw = (timing_raw >> 8) as i8;
        let fps = if fps_raw < 0 { -fps_raw } else { fps_raw } as u8;
        let subframe = (timing_raw & 0xFF) as u8;
        Timing::Timecode(
            crate::primitive::Fps::from_int(fps)
                .ok_or_else(|| crate::Error::new(&crate::ErrorKind::Invalid("invalid FPS")))?,
            subframe
        )
    } else {
        // Ticks per quarter note
        Timing::Metrical(crate::num::u15::new(timing_raw))
    };
    
    let header = Header { format, timing };
    
    // Parse tracks
    let mut tracks = Vec::with_capacity(num_tracks as usize);
    let mut pos = 8 + 4 + header_len; // MThd + length + header data
    
    for _ in 0..num_tracks {
        if pos + 8 > data.len() {
            return Err(crate::Error::new(
                &crate::ErrorKind::Invalid("truncated track header")
            ));
        }
        
        if &data[pos..pos+4] != b"MTrk" {
            return Err(crate::Error::new(
                &crate::ErrorKind::Invalid("expected MTrk chunk")
            ));
        }
        
        let track_len = u32::from_be_bytes([
            data[pos+4], data[pos+5], data[pos+6], data[pos+7]
        ]) as usize;
        
        pos += 8;
        
        if pos + track_len > data.len() {
            return Err(crate::Error::new(
                &crate::ErrorKind::Invalid("truncated track data")
            ));
        }
        
        tracks.push(&data[pos..pos+track_len]);
        pos += track_len;
    }
    
    Ok((header, tracks))
}

fn find_mthd_in_riff(data: &[u8]) -> crate::Result<&[u8]> {
    // Simple RIFF unwrapping - find MThd inside
    for i in 0..data.len().saturating_sub(4) {
        if &data[i..i+4] == b"MThd" {
            return Ok(&data[i..]);
        }
    }
    Err(crate::Error::new(
        &crate::ErrorKind::Invalid("RIFF file does not contain MIDI data")
    ))
}

/// Statistics about a MIDI file without fully parsing it.
pub struct FileStats {
    /// The MIDI file header information
    pub header: Header,
    /// Number of tracks in the file
    pub track_count: usize,
    /// Total number of events across all tracks
    pub total_events: usize,
    /// Total number of note-on events
    pub total_notes: usize,
    /// Total duration in MIDI ticks
    pub total_duration_ticks: u64,
}

impl<'a> MmapSmf<'a> {
    /// Quickly gather statistics without allocating.
    pub fn stats(&self) -> FileStats {
        let mut total_events = 0;
        let mut total_notes = 0;
        let mut total_duration_ticks = 0u64;
        
        for track in &self.tracks {
            let mut track_ticks = 0u64;
            for ev in track.iter() {
                total_events += 1;
                if let Ok(e) = ev {
                    track_ticks += e.delta.as_int() as u64;
                    if let TrackEventKind::Midi { 
                        message: MidiMessage::NoteOn { .. }, ..
                    } = e.kind {
                        total_notes += 1;
                    }
                }
            }
            total_duration_ticks = total_duration_ticks.max(track_ticks);
        }
        
        FileStats {
            header: self.header,
            track_count: self.tracks.len(),
            total_events,
            total_notes,
            total_duration_ticks,
        }
    }
}
