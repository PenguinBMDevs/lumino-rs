use crate::midi::MidiEvent;

use super::types::MidiMemoryManager;

impl MidiMemoryManager {
    /// 获取指定 tick 范围内所有内存中音轨的事件（浏览用，快速）
    pub fn get_in_memory_events_in_range(&self, start_tick: u32, end_tick: u32) -> Vec<&MidiEvent> {
        let mut result = Vec::new();
        for events in self.in_memory_tracks.values() {
            for ev in events {
                let tick = ev.tick();
                if tick >= start_tick && tick < end_tick {
                    result.push(ev);
                }
            }
        }
        result.sort_by_key(|e| e.tick());
        result
    }

    /// 获取所有音轨在指定 tick 范围内的事件（包括磁盘音轨，较慢）
    pub fn get_all_events_in_range(
        &mut self,
        start_tick: u32,
        end_tick: u32,
    ) -> Result<Vec<MidiEvent>, String> {
        let mut result = Vec::new();

        for track_idx in 0..self.track_summaries.len() {
            let events = self.get_track_events(track_idx)?;
            for ev in events {
                let tick = ev.tick();
                if tick >= start_tick && tick < end_tick {
                    result.push(ev.clone());
                }
            }
        }

        result.sort_by_key(|e| e.tick());
        Ok(result)
    }
}
