use crate::midi::managed_midi::{TrackLocationSerde, TrackSummary};

pub fn create_track_summary(
    track_idx: usize,
    event_count: u64,
    note_count: u64,
    high_vel_count: u64,
    max_tick: u32,
    memory_bytes: usize,
    in_memory: bool,
) -> TrackSummary {
    TrackSummary {
        track_index: track_idx,
        event_count,
        note_count,
        high_vel_note_count: high_vel_count,
        max_tick,
        memory_bytes,
        location: if in_memory {
            TrackLocationSerde::InMemory
        } else {
            TrackLocationSerde::OnDisk
        },
    }
}

pub fn decide_track_storage(
    parsed: &super::parser::ParsedTrack,
    track_idx: usize,
    event_count: u64,
    memory_used: &mut usize,
    initial_memory_limit: usize,
) -> (TrackSummary, bool) {
    let should_try_memory = parsed.high_vel_count > 0;

    if !should_try_memory {
        let summary = create_track_summary(
            track_idx,
            event_count,
            parsed.note_count,
            parsed.high_vel_count,
            parsed.max_tick,
            0,
            false,
        );
        return (summary, false);
    }

    let track_size = super::memory::estimate_events_size(&parsed.events);

    if *memory_used + track_size <= initial_memory_limit {
        *memory_used += track_size;
        let summary = create_track_summary(
            track_idx,
            event_count,
            parsed.note_count,
            parsed.high_vel_count,
            parsed.max_tick,
            track_size,
            true,
        );
        (summary, true)
    } else {
        let summary = create_track_summary(
            track_idx,
            event_count,
            parsed.note_count,
            parsed.high_vel_count,
            parsed.max_tick,
            0,
            false,
        );
        (summary, false)
    }
}
