use std::sync::atomic::Ordering;

use crate::midi::managed_midi::{ManagerStats, TrackLocationSerde};

use super::types::MidiMemoryManager;

impl MidiMemoryManager {
    /// 获取统计信息
    pub fn stats(&self) -> ManagerStats {
        let in_memory_count = self.in_memory_tracks.len();
        let on_disk_count = self
            .track_summaries
            .iter()
            .filter(|s| s.location == TrackLocationSerde::OnDisk)
            .count();
        let loaded_count = self.loaded_tracks.len();
        let base_memory = self.memory_used.load(Ordering::Relaxed);
        let total_notes: u64 = self.track_summaries.iter().map(|s| s.note_count).sum();
        let high_vel_notes: u64 = self
            .track_summaries
            .iter()
            .map(|s| s.high_vel_note_count)
            .sum();

        ManagerStats {
            track_count: self.track_summaries.len(),
            in_memory_track_count: in_memory_count,
            on_disk_track_count: on_disk_count,
            loaded_track_count: loaded_count,
            base_memory_bytes: base_memory,
            loaded_memory_bytes: self.loaded_memory_used,
            total_memory_bytes: base_memory + self.loaded_memory_used,
            memory_limit_bytes: self.memory_limit,
            total_notes,
            high_velocity_notes: high_vel_notes,
        }
    }
}
