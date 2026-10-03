//! MIDI event types shared by the parser, the scheduler and the engine.

pub mod parser;
pub mod port;
pub mod stream;

pub use parser::MidiFile;
pub use stream::MidiStream;

/// A MIDI event as understood by the synthesizer (channel-scoped).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEvent {
    /// A note starts. `vel` is the MIDI velocity (1-127).
    NoteOn { key: u8, vel: u8 },
    /// A note stops.
    NoteOff { key: u8 },
    /// A control change. `controller` is the CC number (0-127).
    ControlChange { controller: u8, value: u8 },
    /// A program change (instrument selection).
    ProgramChange { program: u8 },
    /// A pitch bend. `value` is the raw 14-bit value (0-16383, 8192 = center).
    PitchBend { value: u16 },
}

impl MidiEvent {
    /// Returns `true` if this event is a note-on with zero velocity, which
    /// by the MIDI convention is equivalent to a note-off.
    pub fn is_zero_velocity_note_on(&self) -> bool {
        matches!(self, MidiEvent::NoteOn { vel, .. } if *vel == 0)
    }
}

/// Event kind tags for the packed [`TimedEvent`] representation.
pub mod kind {
    pub const NOTE_ON: u32 = 0;
    pub const NOTE_OFF: u32 = 1;
    pub const CONTROL_CHANGE: u32 = 2;
    pub const PROGRAM_CHANGE: u32 = 3;
    pub const PITCH_BEND: u32 = 4;
}

/// A MIDI event bound to an absolute sample position in the output stream,
/// packed into exactly **8 bytes** (down from 16).
///
/// ```text
/// sample (u32) | packed (u32) = channel (8 bits) | kind (4 bits) | payload (20 bits)
/// ```
///
/// `channel` is the **global** MIDI channel `port*16+ch` (0..=255, REND-002
/// #87). The largest payload layout is 16 bits (`key | vel << 8`), so the
/// 20-bit field is lossless for every event kind.
///
/// Payload layouts:
/// - `NoteOn`/`NoteOff`: `key | vel << 8`
/// - `ControlChange`: `controller | value << 8`
/// - `ProgramChange`: `program`
/// - `PitchBend`: `value` (14 bits)
///
/// Why so small: black-MIDI files hold 100-200M events; the old
/// `u64 + u8 + enum` layout cost 16 bytes per event (~3.2 GB for 200M),
/// plus a 16-byte intermediate during parsing - the source of the
/// multi-GB peaks that blew up on "Rekt Apple!!.mid" (800 MB, 201M events).
///
/// The `sample` field is computed from the MIDI tempo map, so events are
/// sample-accurate regardless of tempo changes. `u32` samples cover
/// ~18.6 hours at 64 kHz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedEvent {
    /// Absolute output sample index at which this event is applied.
    pub sample: u32,
    /// `channel (8) | kind (4) | payload (20)` - see the type docs.
    pub packed: u32,
}

impl TimedEvent {
    /// Builds a packed event.
    pub fn new(sample: u32, channel: u8, kind: u32, payload: u32) -> Self {
        Self {
            sample,
            packed: ((channel as u32) << 24) | ((kind & 0xF) << 20) | (payload & 0x000F_FFFF),
        }
    }

    /// Packs a classic [`MidiEvent`] into the compact representation.
    #[inline]
    pub fn from_event(sample: u32, channel: u8, event: MidiEvent) -> Self {
        let (k, p) = match event {
            MidiEvent::NoteOn { key, vel } => (kind::NOTE_ON, key as u32 | ((vel as u32) << 8)),
            MidiEvent::NoteOff { key } => (kind::NOTE_OFF, key as u32),
            MidiEvent::ControlChange { controller, value } => (
                kind::CONTROL_CHANGE,
                controller as u32 | ((value as u32) << 8),
            ),
            MidiEvent::ProgramChange { program } => (kind::PROGRAM_CHANGE, program as u32),
            MidiEvent::PitchBend { value } => (kind::PITCH_BEND, value as u32),
        };
        Self::new(sample, channel, k, p)
    }

    /// The global MIDI channel (0-255; `port*16+ch`, REND-002 #87).
    #[inline]
    pub fn channel(&self) -> u8 {
        (self.packed >> 24) as u8
    }

    /// The packed event kind (see [`kind`]).
    #[inline]
    pub fn kind(&self) -> u32 {
        (self.packed >> 20) & 0xF
    }

    /// The 20-bit event payload (layout depends on [`Self::kind`]).
    #[inline]
    pub fn payload(&self) -> u32 {
        self.packed & 0x000F_FFFF
    }

    /// Decodes the event payload into the classic enum view.
    #[inline]
    pub fn event(&self) -> MidiEvent {
        let p = self.payload();
        match self.kind() {
            kind::NOTE_ON => MidiEvent::NoteOn {
                key: p as u8,
                vel: (p >> 8) as u8,
            },
            kind::NOTE_OFF => MidiEvent::NoteOff { key: p as u8 },
            kind::CONTROL_CHANGE => MidiEvent::ControlChange {
                controller: p as u8,
                value: (p >> 8) as u8,
            },
            kind::PROGRAM_CHANGE => MidiEvent::ProgramChange { program: p as u8 },
            kind::PITCH_BEND => MidiEvent::PitchBend { value: p as u16 },
            _ => unreachable!("invalid packed kind: {}", self.kind()),
        }
    }

    /// Note key for note-on/note-off events.
    #[inline]
    pub fn note_key(&self) -> u8 {
        self.payload() as u8
    }

    /// Velocity for note-on events.
    #[inline]
    pub fn note_vel(&self) -> u8 {
        (self.payload() >> 8) as u8
    }
}

/// A parsed MIDI sequence: the full event stream with sample-accurate
/// timestamps, ready to be consumed by the engine.
///
/// Obtained via [`crate::MidiFile::load`].
#[derive(Debug, Clone, PartialEq)]
pub struct MidiSequence {
    /// All events in ascending sample order (events from every track and
    /// channel are merged; `channel` is the global `port*16+ch`).
    pub events: Vec<TimedEvent>,
    /// The output sample position of the last event (the MIDI's end).
    pub end_sample: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_event_roundtrips_full_global_channel_range() {
        for ch in [0u8, 1, 15, 16, 31, 127, 254, 255] {
            for (k, payload) in [
                (kind::NOTE_ON, 60u32 | (100 << 8)),
                (kind::NOTE_OFF, 60u32),
                (kind::CONTROL_CHANGE, 7u32 | (127 << 8)),
                (kind::PROGRAM_CHANGE, 42u32),
                (kind::PITCH_BEND, 8192u32),
            ] {
                let ev = TimedEvent::new(1234, ch, k, payload);
                assert_eq!(ev.sample, 1234);
                assert_eq!(ev.channel(), ch);
                assert_eq!(ev.kind(), k);
                assert_eq!(ev.payload(), payload);
            }
        }
    }

    #[test]
    fn port_global_channel_matches_cpu_semantics() {
        assert_eq!(port::global_channel(0, 9), 9);
        assert_eq!(port::global_channel(1, 0), 16);
        assert_eq!(port::global_channel(6, 15), 111);
        assert_eq!(port::global_channel(15, 15), 255);
        assert_eq!(port::global_channel(16, 0), 240, "超限端口折叠到 15 块");
        assert_eq!(port::global_channel(127, 15), 255);
    }
}
