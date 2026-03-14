//! 内存管理的 MIDI 加载器
//!
//! 设计原则：
//! - 内存上限 1GB，超出后数据溢出到磁盘缓存
//! - 力度(velocity) > 1 的音符事件优先保留在内存中
//! - velocity ≤ 1 的音符不保留在内存区域
//! - 含有被内存保留的音符的音轨，其非音符事件也在内存中
//! - 其余音轨的事件按音轨顺序写入磁盘缓存
//! - 编辑和浏览时，按需从磁盘加载
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义（TrackLocation, TrackSummary, ManagerStats 等）
//! - `cache`: 磁盘缓存管理
//! - `loader`: 加载辅助函数
//! - `manager`: MidiMemoryManager 主结构

use crate::midi::MidiEvent;

pub mod cache;
pub mod loader;
pub mod manager;
pub mod types;

pub use cache::DiskTrackCache;
pub use loader::{
    create_track_summary, estimate_event_size, estimate_events_size, load_midi_data,
    parse_track_events_from_iter, spawn_disk_writer,
};
pub use manager::MidiMemoryManager;
pub use types::{ManagerStats, TrackLocation, TrackLocationSerde, TrackSummary};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_event_size() {
        let note_on = MidiEvent::NoteOn {
            track: 0,
            tick: 100,
            channel: 0,
            key: 60,
            velocity: 100,
        };
        assert!(estimate_event_size(&note_on) > 0);
    }
}
