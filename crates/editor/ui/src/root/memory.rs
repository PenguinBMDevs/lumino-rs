//! Root 内存占用快照收集

use crate::root::{MemoryBreakdown, Root};

impl Root {
    /// 收集各组件的内存占用快照
    pub fn memory_breakdown(&self) -> MemoryBreakdown {
        let editor_mem = self.editor.memory_breakdown();

        // track_midi_events: HashMap<usize, Vec<MidiTrackEvent>>
        let track_midi_events_entries = self.playback.track_midi_events.len();
        let track_midi_events_bytes = self
            .playback
            .track_midi_events
            .values()
            .map(|v| v.capacity() * std::mem::size_of::<crate::playback::MidiTrackEvent>())
            .sum();

        MemoryBreakdown {
            editor: editor_mem,
            track_midi_events_entries,
            track_midi_events_bytes,
            ..Default::default()
        }
    }
}
