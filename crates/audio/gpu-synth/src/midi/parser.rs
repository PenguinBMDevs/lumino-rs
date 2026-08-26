//! MIDI file parsing on top of `lumino-midly`.
//!
//! [`MidiFile`] converts a standard MIDI file into a [`MidiSequence`] of
//! sample-accurate events, honoring tempo changes and merging all tracks.

use lumino_midly::num::u24;
use lumino_midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use crate::SynthError;
use crate::midi::kind;
use crate::midi::{MidiSequence, TimedEvent};

/// A parsed MIDI file.
///
/// # Example
///
/// ```
/// use lumino_gpu_synth::MidiFile;
///
/// let midi = MidiFile::load("assets/right-example.mid", 64_000).unwrap();
/// assert_eq!(midi.sample_rate, 64_000);
/// assert!(midi.sequence.events.len() >= 5);
/// ```
#[derive(Debug, Clone)]
pub struct MidiFile {
    /// The parsed, sample-accurate event sequence.
    pub sequence: MidiSequence,
    /// The sample rate the sequence was computed for.
    pub sample_rate: u32,
    /// Tempo events (tick position, microseconds per quarter note).
    pub tempos: Vec<(u64, u32)>,
    /// Total length of the song in ticks.
    pub length_ticks: u64,
}

impl MidiFile {
    /// Parses a MIDI file from disk and builds a sample-accurate event
    /// sequence at `sample_rate` Hz.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the file is not a valid MIDI file, and
    /// [`SynthError::Io`] on read failures.
    pub fn load(path: impl AsRef<std::path::Path>, sample_rate: u32) -> Result<Self, SynthError> {
        let raw = std::fs::read(path)?;
        Self::parse(&raw, sample_rate)
    }

    /// Parses a MIDI file from raw bytes at `sample_rate` Hz.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the data is not a valid MIDI file.
    pub fn parse(raw: &[u8], sample_rate: u32) -> Result<Self, SynthError> {
        let smf = Smf::parse(raw)
            .map_err(|e| SynthError::Midi(format!("lumino-midly failed to parse: {e}")))?;

        let ticks_per_beat = match smf.header.timing {
            Timing::Metrical(ppq) => ppq.as_int() as u64,
            Timing::Timecode(_, _) => {
                return Err(SynthError::Midi(
                    "SMPTE timecode timing is not supported".into(),
                ));
            }
        };
        if ticks_per_beat == 0 {
            return Err(SynthError::Midi("zero ticks per beat".into()));
        }

        // ---- pass 1: tempo map --------------------------------------------
        // Tempo events are few (even in automated files); collect them
        // first so pass 2 can convert ticks -> samples directly without a
        // second event list sitting in memory.
        let mut tempos: Vec<(u64, u32)> = Vec::new();
        let mut length_ticks: u64 = 0;
        for track in &smf.tracks {
            let mut tick: u64 = 0;
            for ev in track {
                tick += ev.delta.as_int() as u64;
                length_ticks = length_ticks.max(tick);
                if let TrackEventKind::Meta(MetaMessage::Tempo(us_per_beat)) = &ev.kind {
                    tempos.push((tick, u24_to_u32(*us_per_beat)));
                }
            }
        }

        // Cumulative seconds per tempo segment; tick -> seconds is a binary
        // search (large automated files can contain hundreds of thousands of
        // tempo events; a linear scan per event would be O(events x tempos)).
        // Default tempo is 500_000 us/beat (120 BPM).
        let mut tempo_segs: Vec<(u64, f64, f64)> = Vec::with_capacity(tempos.len() + 1);
        let mut prev_tick = 0u64;
        let mut prev_tempo = 500_000.0;
        let mut cum_secs = 0.0f64;
        for &(tick, us) in tempos.iter() {
            tempo_segs.push((prev_tick, cum_secs, prev_tempo));
            cum_secs +=
                (tick - prev_tick) as f64 * prev_tempo / 1_000_000.0 / ticks_per_beat as f64;
            prev_tick = tick;
            prev_tempo = us as f64;
        }
        tempo_segs.push((prev_tick, cum_secs, prev_tempo));

        let ticks_to_sample = |tick: u64| -> u32 {
            let i = tempo_segs
                .partition_point(|&(start_tick, _, _)| start_tick <= tick)
                .saturating_sub(1);
            let (start_tick, cum, us) = tempo_segs[i];
            let sec = cum + (tick - start_tick) as f64 * us / 1_000_000.0 / ticks_per_beat as f64;
            (sec * sample_rate as f64).round() as u32
        };

        // ---- pass 2: build the packed event stream directly --------------
        // No intermediate `(tick, channel, MidiEvent)` list: black-MIDI
        // files hold 100-200M events, and a second 16-byte-per-event list
        // would double the peak memory (multi-GB on "Rekt Apple!!.mid").
        // Events are appended to the final Vec as they are read, so the
        // only big allocations are the final 8-byte-per-event array and
        // midly's own parse tree (freed when `smf` drops below).
        let mut events: Vec<TimedEvent> = Vec::new();
        for track in &smf.tracks {
            let mut tick: u64 = 0;
            for ev in track {
                tick += ev.delta.as_int() as u64;
                let TrackEventKind::Midi { channel, message } = &ev.kind else {
                    continue;
                };
                let channel = channel.as_int();
                let (k, payload) = match *message {
                    MidiMessage::NoteOn { key, vel } => {
                        let vel = vel.as_int();
                        if vel == 0 {
                            (kind::NOTE_OFF, key as u32)
                        } else {
                            (kind::NOTE_ON, key as u32 | ((vel as u32) << 8))
                        }
                    }
                    MidiMessage::NoteOff { key, .. } => (kind::NOTE_OFF, key as u32),
                    MidiMessage::Controller { controller, value } => (
                        kind::CONTROL_CHANGE,
                        controller.as_int() as u32 | ((value.as_int() as u32) << 8),
                    ),
                    MidiMessage::ProgramChange { program } => {
                        (kind::PROGRAM_CHANGE, program.as_int() as u32)
                    }
                    MidiMessage::PitchBend { bend } => (kind::PITCH_BEND, bend.0.as_int() as u32),
                    _ => continue,
                };
                events.push(TimedEvent::new(ticks_to_sample(tick), channel, k, payload));
            }
        }

        // Stable sort by sample only: events at the same tick keep the
        // original MIDI order (track order, then per-track order), exactly
        // like XSynth's merged track iterator. Sorting by channel as well
        // would reorder same-tick note-on/note-off pairs across tracks and
        // change note lifetimes relative to the reference.
        events.sort_by_key(|e| e.sample);

        let end_sample = ticks_to_sample(length_ticks) as u64;

        Ok(Self {
            sequence: MidiSequence { events, end_sample },
            sample_rate,
            tempos,
            length_ticks,
        })
    }

    /// Returns the sequence length in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.sequence.end_sample as f64 / self.sample_rate as f64
    }

    /// Writes this MIDI back to a file (mostly useful for debugging).
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), SynthError> {
        let _ = (path, self);
        // Note: full SMF re-serialization is intentionally not implemented;
        // this method exists as a placeholder for tooling.
        Err(SynthError::Config(
            "MidiFile::save is not implemented; use lumino-midly directly".into(),
        ))
    }
}

fn u24_to_u32(v: u24) -> u32 {
    v.as_int()
}
