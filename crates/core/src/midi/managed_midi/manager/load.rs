use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicUsize;

use crate::midi::managed_midi::{TrackSummary, load_midi_data};

use super::types::MidiMemoryManager;

impl MidiMemoryManager {
    /// 从 MIDI 文件构建内存管理器（流式逐轨解析 + 并行磁盘写入）
    ///
    /// 优化要点：
    /// 1. 使用 `midly::parse()` 的懒 `TrackIter`，逐个音轨解析，内存中同时只有一个音轨的事件
    /// 2. 磁盘写入在后台线程并行执行（主线程继续处理下一个音轨）
    /// 3. 过滤失败（内存满）时直接用原始 events 写磁盘，不重新打开文件
    /// 4. 避免 `Smf::parse()` 一次性解析整个文件——764MB 文件也不会 OOM
    pub fn load(
        source_path: &Path,
        cache_base_dir: &Path,
        progress_callback: Option<&dyn Fn(f64)>,
        max_ram_bytes: Option<usize>,
    ) -> Result<Self, String> {
        let loaded_data = load_midi_data(
            source_path,
            cache_base_dir,
            progress_callback,
            max_ram_bytes,
        )?;

        Ok(Self {
            in_memory_tracks: loaded_data.in_memory_tracks,
            loaded_tracks: HashMap::new(),
            track_summaries: loaded_data.summaries,
            disk_cache: loaded_data.disk_cache,
            memory_used: AtomicUsize::new(loaded_data.memory_used),
            memory_limit: max_ram_bytes.unwrap_or(1024 * 1024 * 1024),
            lru_order: Vec::new(),
            loaded_memory_limit: loaded_data.loaded_memory_limit,
            loaded_memory_used: 0,
            source_path: source_path.to_path_buf(),
        })
    }

    /// 动态设置内存上限（字节）
    ///
    /// 注意：这不会立即触发内存回收或重新加载，仅影响后续的加载行为。
    pub fn set_memory_limit(&mut self, limit_bytes: usize) {
        self.memory_limit = limit_bytes;
        self.loaded_memory_limit = limit_bytes / 4;
    }

    /// 解析单个 TrackEventKind 为 MidiEvent
    pub fn parse_track_event(
        track_index: usize,
        tick: u32,
        kind: &midly::TrackEventKind,
    ) -> Option<crate::midi::MidiEvent> {
        use midly::{MetaMessage, TrackEventKind};

        match kind {
            TrackEventKind::Meta(MetaMessage::EndOfTrack) => None,
            _ => crate::midi::event::parse_track_event_kind(track_index, tick, kind),
        }
    }

    /// 获取音轨数量
    pub fn track_count(&self) -> usize {
        self.track_summaries.len()
    }

    /// 获取音轨摘要信息
    pub fn track_summary(&self, track_index: usize) -> Option<&TrackSummary> {
        self.track_summaries.get(track_index)
    }

    /// 获取所有音轨摘要
    pub fn all_summaries(&self) -> &[TrackSummary] {
        &self.track_summaries
    }

    /// 获取当前内存使用量（字节）
    pub fn memory_used(&self) -> usize {
        self.memory_used.load(std::sync::atomic::Ordering::Relaxed) + self.loaded_memory_used
    }

    /// 获取内存上限（字节）
    pub fn memory_limit(&self) -> usize {
        self.memory_limit
    }

    /// 获取源文件路径
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }
}
