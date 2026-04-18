use crate::midi::MidiEvent;

const MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;
const PROGRESS_START: f64 = 0.01;
const PROGRESS_MAIN_RATIO: f64 = 0.94;

pub fn estimate_event_size(event: &MidiEvent) -> usize {
    match event {
        MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. } => 24,
        MidiEvent::ControlChange { .. } => 24,
        MidiEvent::ProgramChange { .. } => 16,
        MidiEvent::Tempo { .. } => 16,
        MidiEvent::TimeSignature { .. } => 16,
        MidiEvent::KeySignature { .. } => 16,
        MidiEvent::TrackName { name, .. } => 24 + name.len(),
        MidiEvent::Other { raw, .. } => 24 + raw.len(),
    }
}

pub fn estimate_events_size(events: &[MidiEvent]) -> usize {
    let mut total = 24usize;
    for e in events {
        total += estimate_event_size(e);
    }
    total
}

pub fn get_progress_start() -> f64 {
    PROGRESS_START
}

pub fn get_progress_main_ratio() -> f64 {
    PROGRESS_MAIN_RATIO
}

pub fn get_memory_limit_bytes() -> usize {
    MEMORY_LIMIT_BYTES
}
