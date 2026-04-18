use crate::midi::MidiEvent;
use crate::midi::managed_midi::MidiMemoryManager;

pub struct ParsedTrack {
    pub events: Vec<MidiEvent>,
    pub note_count: u64,
    pub high_vel_count: u64,
    pub max_tick: u32,
}

pub fn parse_track_events_from_iter(
    track_idx: usize,
    event_iter: midly::EventIter,
) -> Result<ParsedTrack, String> {
    let mut events = Vec::new();
    let mut current_tick = 0u32;
    let mut note_count = 0u64;
    let mut high_vel_count = 0u64;
    let mut max_tick = 0u32;

    for event_result in event_iter {
        let track_event =
            event_result.map_err(|e| format!("解析音轨 {} 事件失败: {e}", track_idx))?;

        current_tick = current_tick.saturating_add(u32::from(track_event.delta));

        if let Some(midi_event) =
            MidiMemoryManager::parse_track_event(track_idx, current_tick, &track_event.kind)
        {
            if current_tick > max_tick {
                max_tick = current_tick;
            }
            if let MidiEvent::NoteOn { velocity, .. } = &midi_event
                && *velocity > 0
            {
                note_count += 1;
                if *velocity > 1 {
                    high_vel_count += 1;
                }
            }
            events.push(midi_event);
        }
    }

    Ok(ParsedTrack {
        events,
        note_count,
        high_vel_count,
        max_tick,
    })
}
