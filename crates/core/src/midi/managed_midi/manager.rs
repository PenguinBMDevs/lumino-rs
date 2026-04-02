//! MIDI 内存管理器

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::midi::MidiEvent;
use crate::midi::managed_midi::loader::estimate_events_size;
use crate::midi::managed_midi::{
    DiskTrackCache, ManagerStats, TrackLocationSerde, TrackSummary, load_midi_data,
};

/// 内存管理的 MIDI 数据
///
/// 按优先级将音轨事件分为内存区和磁盘区：
/// - 含有 velocity > 1 音符的音轨 → 内存
/// - 其余音轨 → 磁盘缓存
/// - 总内存不超过 1GB
#[derive(Debug)]
pub struct MidiMemoryManager {
    /// 内存中的音轨事件 (track_index → events)
    in_memory_tracks: HashMap<usize, Vec<MidiEvent>>,
    /// 从磁盘按需加载后暂留内存的音轨
    loaded_tracks: HashMap<usize, Vec<MidiEvent>>,
    /// 各音轨的摘要
    track_summaries: Vec<TrackSummary>,
    /// 磁盘缓存
    disk_cache: DiskTrackCache,
    /// 当前内存使用量（字节）
    memory_used: AtomicUsize,
    /// 内存上限
    memory_limit: usize,
    /// 按需加载的音轨的 LRU 访问顺序
    lru_order: Vec<usize>,
    /// 按需加载区的内存上限（预留总上限的 25% 给按需加载）
    loaded_memory_limit: usize,
    /// 按需加载区当前内存使用量
    loaded_memory_used: usize,
    /// 源文件路径
    source_path: PathBuf,
}

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
        let (disk_cache, summaries, in_memory_tracks, memory_used, loaded_memory_limit) =
            load_midi_data(
                source_path,
                cache_base_dir,
                progress_callback,
                max_ram_bytes,
            )?;

        Ok(Self {
            in_memory_tracks,
            loaded_tracks: HashMap::new(),
            track_summaries: summaries,
            disk_cache,
            memory_used: AtomicUsize::new(memory_used),
            memory_limit: max_ram_bytes.unwrap_or(1024 * 1024 * 1024),
            lru_order: Vec::new(),
            loaded_memory_limit,
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
    ) -> Option<MidiEvent> {
        use midly::{MetaMessage, TrackEventKind};

        // 使用公共的解析函数，并过滤掉 EndOfTrack
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
        self.memory_used.load(Ordering::Relaxed) + self.loaded_memory_used
    }

    /// 获取内存上限（字节）
    pub fn memory_limit(&self) -> usize {
        self.memory_limit
    }

    /// 获取源文件路径
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// 获取音轨事件（编辑/浏览用）
    ///
    /// 如果在内存中，直接返回引用；
    /// 如果在磁盘上，加载到 loaded_tracks 中（受 LRU 管理），再返回引用。
    pub fn get_track_events(&mut self, track_index: usize) -> Result<&[MidiEvent], String> {
        if track_index >= self.track_summaries.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        // 先检查是否在内存中
        if self.in_memory_tracks.contains_key(&track_index) {
            // 安全：contains_key 检查通过后 get 一定返回 Some
            return Ok(self
                .in_memory_tracks
                .get(&track_index)
                .ok_or(format!("音轨 {} 内存数据意外丢失", track_index))?);
        }

        // 检查是否已经从磁盘加载
        if self.loaded_tracks.contains_key(&track_index) {
            // 更新 LRU（必须在 get 之前，避免借用冲突）
            self.touch_lru(track_index);
            return Ok(self
                .loaded_tracks
                .get(&track_index)
                .ok_or(format!("音轨 {} 加载数据意外丢失", track_index))?);
        }

        // 需要从磁盘加载
        let events = self
            .disk_cache
            .read_track(track_index)
            .map_err(|e| format!("从磁盘加载音轨 {} 失败: {e}", track_index))?;

        let event_size = estimate_events_size(&events);

        // 如果加载后超过按需加载内存限制，先淘汰最旧的
        while self.loaded_memory_used + event_size > self.loaded_memory_limit
            && !self.lru_order.is_empty()
        {
            self.evict_oldest_loaded();
        }

        self.loaded_memory_used += event_size;
        self.loaded_tracks.insert(track_index, events);
        self.lru_order.push(track_index);
        // insert 后立即 get 一定存在
        Ok(self
            .loaded_tracks
            .get(&track_index)
            .ok_or(format!("音轨 {} 插入后数据意外丢失", track_index))?)
    }

    /// 获取音轨事件的完整数据（用于编辑，需要可变访问）
    ///
    /// 所有音轨的完整数据都在磁盘上（包括 InMemory 的音轨）。
    /// InMemory 中只有过滤后的数据，编辑需要完整数据。
    pub fn get_track_events_full(&mut self, track_index: usize) -> Result<Vec<MidiEvent>, String> {
        if track_index >= self.track_summaries.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        // 所有音轨的完整数据都写入了磁盘缓存
        if self.disk_cache.has_track(track_index) {
            let events = self
                .disk_cache
                .read_track(track_index)
                .map_err(|e| format!("从磁盘加载音轨 {} 失败: {e}", track_index))?;
            return Ok(events);
        }

        // 回退：如果磁盘上没有（理论上不应该发生）
        if let Some(events) = self.in_memory_tracks.get(&track_index) {
            return Ok(events.clone());
        }

        Err(format!("音轨 {} 数据不存在", track_index))
    }

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

    /// 卸载所有按需加载的音轨
    pub fn unload_all_loaded(&mut self) {
        let freed: usize = self
            .loaded_tracks
            .values()
            .map(|e| estimate_events_size(e))
            .sum();
        self.loaded_tracks.clear();
        self.lru_order.clear();
        self.loaded_memory_used = self.loaded_memory_used.saturating_sub(freed);
    }

    /// 卸载指定按需加载的音轨
    pub fn unload_track(&mut self, track_index: usize) {
        if let Some(events) = self.loaded_tracks.remove(&track_index) {
            let size = estimate_events_size(&events);
            self.loaded_memory_used = self.loaded_memory_used.saturating_sub(size);
            self.lru_order.retain(|&i| i != track_index);
        }
    }

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

    fn touch_lru(&mut self, track_index: usize) {
        if let Some(pos) = self.lru_order.iter().position(|&i| i == track_index) {
            self.lru_order.remove(pos);
        }
        self.lru_order.push(track_index);
    }

    fn evict_oldest_loaded(&mut self) {
        if let Some(oldest) = self.lru_order.first().copied() {
            if let Some(events) = self.loaded_tracks.remove(&oldest) {
                let size = estimate_events_size(&events);
                self.loaded_memory_used = self.loaded_memory_used.saturating_sub(size);
            }
            // 无论 remove 是否成功，都移除 lru_order 首元素以保持一致性
            self.lru_order.remove(0);
        }
    }
}
