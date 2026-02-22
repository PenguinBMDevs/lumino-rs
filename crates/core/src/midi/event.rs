use std::fs::File;
use memmap2::Mmap;
use midly::{Smf, MidiMessage, MetaMessage, TrackEventKind};

/// MIDI 事件类型（轻量级表示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MidiEvent {
    NoteOn { track: usize, tick: u32, channel: u8, key: u8, velocity: u8 },
    NoteOff { track: usize, tick: u32, channel: u8, key: u8, velocity: u8 },
    ControlChange { track: usize, tick: u32, channel: u8, controller: u8, value: u8 },
    ProgramChange { track: usize, tick: u32, channel: u8, program: u8 },
    Tempo { track: usize, tick: u32, tempo: u32 },
    TimeSignature { track: usize, tick: u32, numerator: u8, denominator: u8 },
    KeySignature { track: usize, tick: u32, key: i8, is_major: bool },
    TrackName { track: usize, tick: u32, name: String },
    Other { track: usize, tick: u32, raw: Vec<u8> },
}

impl MidiEvent {
    pub fn tick(&self) -> u32 {
        match self {
            MidiEvent::NoteOn { tick, .. } => *tick,
            MidiEvent::NoteOff { tick, .. } => *tick,
            MidiEvent::ControlChange { tick, .. } => *tick,
            MidiEvent::ProgramChange { tick, .. } => *tick,
            MidiEvent::Tempo { tick, .. } => *tick,
            MidiEvent::TimeSignature { tick, .. } => *tick,
            MidiEvent::KeySignature { tick, .. } => *tick,
            MidiEvent::TrackName { tick, .. } => *tick,
            MidiEvent::Other { tick, .. } => *tick,
        }
    }
}

/// MIDI 事件流（使用内存映射，零拷贝）
///
/// 内存占用：仅操作系统页面缓存，不占用用户空间内存
pub struct MidiEventStream {
    _file: File,
    _mmap: Mmap,
    smf: Smf<'static>,
    track_index: usize,
    event_index: usize,
    current_tick: u32,
}

impl MidiEventStream {
    pub fn from_path(path: &std::path::Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("打开文件失败: {e}"))?;

        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| format!("内存映射失败: {e}"))?
        };

        let smf = Smf::parse(&mmap[..]).map_err(|e| format!("解析MIDI失败: {e}"))?;

        let smf_static: Smf<'static> = unsafe {
            std::mem::transmute(smf)
        };

        Ok(Self {
            _file: file,
            _mmap: mmap,
            smf: smf_static,
            track_index: 0,
            event_index: 0,
            current_tick: 0,
        })
    }

    pub fn track_count(&self) -> usize {
        self.smf.tracks.len()
    }

    pub fn division(&self) -> u16 {
        match self.smf.header.timing {
            midly::Timing::Metrical(ticks) => ticks.as_int(),
            _ => 480,
        }
    }

    pub fn read_track_events(&mut self, track_index: usize) -> Result<Vec<MidiEvent>, String> {
        if track_index >= self.smf.tracks.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        let track = &self.smf.tracks[track_index];
        let mut events = Vec::with_capacity(track.len());
        let mut current_tick = 0u32;

        for event in track {
            current_tick = current_tick.saturating_add(u32::from(event.delta));

            let midi_event = self.parse_event(track_index, current_tick, &event.kind);
            if let Some(evt) = midi_event {
                events.push(evt);
            }
        }

        Ok(events)
    }

    pub fn scan_track_for_stats<F>(&self, track_index: usize, mut callback: F) -> Result<u32, String>
    where
        F: FnMut(bool, u32),
    {
        if track_index >= self.smf.tracks.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        let track = &self.smf.tracks[track_index];
        let mut current_tick = 0u32;
        let mut max_tick = 0u32;

        for event in track {
            current_tick = current_tick.saturating_add(u32::from(event.delta));

            match &event.kind {
                TrackEventKind::Midi { channel: _, message } => {
                    match message {
                        MidiMessage::NoteOn { key: _, vel } => {
                            let is_note_on = vel.as_int() > 0;
                            callback(is_note_on, current_tick);
                        }
                        MidiMessage::NoteOff { .. } => {
                            callback(false, current_tick);
                        }
                        MidiMessage::Controller { .. } |
                        MidiMessage::ProgramChange { .. } => {
                            callback(false, current_tick);
                        }
                        _ => {}
                    }
                }
                TrackEventKind::Meta(meta) => {
                    match meta {
                        MetaMessage::Tempo { .. } |
                        MetaMessage::TimeSignature { .. } |
                        MetaMessage::KeySignature { .. } => {
                            callback(false, current_tick);
                        }
                        MetaMessage::EndOfTrack => {
                            max_tick = current_tick;
                            break;
                        }
                        _ => {}
                    }
                }
                TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => {}
            }
        }

        Ok(max_tick)
    }

    fn parse_event(&self, track_index: usize, tick: u32, kind: &TrackEventKind) -> Option<MidiEvent> {
        match kind {
            TrackEventKind::Midi { channel, message } => {
                let ch = channel.as_int();
                match message {
                    MidiMessage::NoteOn { key, vel } => Some(MidiEvent::NoteOn {
                        track: track_index,
                        tick,
                        channel: ch,
                        key: key.as_int(),
                        velocity: vel.as_int(),
                    }),
                    MidiMessage::NoteOff { key, vel } => Some(MidiEvent::NoteOff {
                        track: track_index,
                        tick,
                        channel: ch,
                        key: key.as_int(),
                        velocity: vel.as_int(),
                    }),
                    MidiMessage::Controller { controller, value } => Some(MidiEvent::ControlChange {
                        track: track_index,
                        tick,
                        channel: ch,
                        controller: controller.as_int(),
                        value: value.as_int(),
                    }),
                    MidiMessage::ProgramChange { program } => Some(MidiEvent::ProgramChange {
                        track: track_index,
                        tick,
                        channel: ch,
                        program: program.as_int(),
                    }),
                    _ => None,
                }
            }
            TrackEventKind::Meta(meta) => {
                match meta {
                    MetaMessage::Tempo(tempo) => Some(MidiEvent::Tempo {
                        track: track_index,
                        tick,
                        tempo: tempo.as_int(),
                    }),
                    MetaMessage::TimeSignature(num, den, _, _) => Some(MidiEvent::TimeSignature {
                        track: track_index,
                        tick,
                        numerator: *num,
                        denominator: *den,
                    }),
                    MetaMessage::KeySignature(key, is_major) => Some(MidiEvent::KeySignature {
                        track: track_index,
                        tick,
                        key: *key,
                        is_major: *is_major,
                    }),
                    MetaMessage::TrackName(name) => Some(MidiEvent::TrackName {
                        track: track_index,
                        tick,
                        name: String::from_utf8_lossy(name).to_string(),
                    }),
                    MetaMessage::EndOfTrack => None,
                    _ => None,
                }
            }
            TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => None,
        }
    }

    fn next_event(&mut self) -> Option<Result<MidiEvent, String>> {
        loop {
            if self.track_index >= self.smf.tracks.len() {
                return None;
            }

            let track = &self.smf.tracks[self.track_index];

            if self.event_index >= track.len() {
                self.track_index += 1;
                self.event_index = 0;
                self.current_tick = 0;
                continue;
            }

            let event = &track[self.event_index];
            self.event_index += 1;
            self.current_tick = self.current_tick.saturating_add(u32::from(event.delta));

            let track_idx = self.track_index;
            let tick = self.current_tick;

            if let Some(midi_event) = self.parse_event(track_idx, tick, &event.kind) {
                return Some(Ok(midi_event));
            }
        }
    }
}

impl Iterator for MidiEventStream {
    type Item = Result<MidiEvent, String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event()
    }
}

/// 解析全部MIDI事件（使用内存映射，低内存）
pub fn parse_all_midi_events(path: &std::path::Path) -> Result<MidiEventStream, String> {
    MidiEventStream::from_path(path)
}
