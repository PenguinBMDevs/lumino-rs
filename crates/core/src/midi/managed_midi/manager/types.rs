use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use crate::midi::managed_midi::{DiskTrackCache, TrackSummary};

/// 内存管理的 MIDI 数据
///
/// 按优先级将音轨事件分为内存区和磁盘区：
/// - 含有 velocity > 1 音符的音轨 → 内存
/// - 其余音轨 → 磁盘缓存
/// - 总内存不超过 1GB
#[derive(Debug)]
pub struct MidiMemoryManager {
    pub(crate) in_memory_tracks: HashMap<usize, Vec<crate::midi::MidiEvent>>,
    pub(crate) loaded_tracks: HashMap<usize, Vec<crate::midi::MidiEvent>>,
    pub(crate) track_summaries: Vec<TrackSummary>,
    pub(crate) disk_cache: DiskTrackCache,
    pub(crate) memory_used: AtomicUsize,
    pub(crate) memory_limit: usize,
    pub(crate) lru_order: Vec<usize>,
    pub(crate) loaded_memory_limit: usize,
    pub(crate) loaded_memory_used: usize,
    pub(crate) source_path: PathBuf,
}
