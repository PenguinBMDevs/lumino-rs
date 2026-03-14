//! 内存管理的 MIDI 加载器
//!
//! 设计原则：
//! - 内存上限 1GB，超出后数据溢出到磁盘缓存
//! - 力度(velocity) > 1 的音符事件优先保留在内存中
//! - velocity ≤ 1 的音符不保留在内存区域
//! - 含有被内存保留的音符的音轨，其非音符事件也在内存中
//! - 其余音轨的事件按音轨顺序写入磁盘缓存
//! - 编辑和浏览时，按需从磁盘加载

use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use crate::midi::MidiEvent;

/// 1 GB 内存上限（字节）
const MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;

use crate::midi::constants::DEFAULT_PPQN;

/// 压缩级别 (1-22, 越高压缩率越好但越慢)
const COMPRESSION_LEVEL: i32 = 3;

/// 进度回调起始值 (1%)
const PROGRESS_START: f64 = 0.01;

/// 进度回调主要部分占比 (94%)
const PROGRESS_MAIN_RATIO: f64 = 0.94;

/// 估算单个 MidiEvent 在内存中的大小（字节）
fn estimate_event_size(event: &MidiEvent) -> usize {
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

/// 估算事件列表的内存占用
fn estimate_events_size(events: &[MidiEvent]) -> usize {
    // Vec 本身开销 (ptr + len + cap) ≈ 24 bytes
    let mut total = 24usize;
    for e in events {
        total += estimate_event_size(e);
    }
    total
}

/// 音轨在内存中的存储状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackLocation {
    /// 全部在内存中（含力度>1的音符的音轨）
    InMemory,
    /// 全部在磁盘上
    OnDisk,
    /// 按需加载后暂留内存
    LoadedFromDisk,
}

/// 单个音轨的摘要信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackSummary {
    pub track_index: usize,
    pub event_count: u64,
    pub note_count: u64,
    /// 力度 > 1 的音符数
    pub high_vel_note_count: u64,
    pub max_tick: u32,
    pub memory_bytes: usize,
    pub location: TrackLocationSerde,
}

/// 可序列化的音轨位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrackLocationSerde {
    InMemory,
    OnDisk,
}

/// 磁盘缓存管理器
#[derive(Debug)]
pub struct DiskTrackCache {
    cache_dir: PathBuf,
    cache_key: u64,
}

impl DiskTrackCache {
    pub fn new(cache_base_dir: &Path, source_path: &Path) -> std::io::Result<Self> {
        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_key(source_path, modified);

        let cache_dir = cache_base_dir.join(format!("managed_{:016x}", key));
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        Ok(Self {
            cache_dir,
            cache_key: key,
        })
    }

    fn compute_key(path: &Path, modified: std::time::SystemTime) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.to_string_lossy().hash(&mut hasher);
        modified.hash(&mut hasher);
        hasher.finish()
    }

    /// 将音轨事件写入磁盘
    pub fn write_track(&self, track_index: usize, events: &[MidiEvent]) -> std::io::Result<()> {
        let track_path = self.track_path(track_index);
        let serialized = bincode::serialize(events).map_err(std::io::Error::other)?;
        let compressed = zstd::stream::encode_all(&mut &serialized[..], COMPRESSION_LEVEL)
            .map_err(std::io::Error::other)?;
        let mut file = File::create(&track_path)?;
        file.write_all(&compressed)?;
        file.sync_all()?;
        Ok(())
    }

    /// 从磁盘加载音轨事件
    pub fn read_track(&self, track_index: usize) -> std::io::Result<Vec<MidiEvent>> {
        let track_path = self.track_path(track_index);
        if !track_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("磁盘缓存中找不到音轨 {}", track_index),
            ));
        }
        let file = File::open(&track_path)?;
        let reader = BufReader::new(file);
        let decompressed = zstd::stream::decode_all(reader).map_err(std::io::Error::other)?;
        let events: Vec<MidiEvent> =
            bincode::deserialize(&decompressed).map_err(std::io::Error::other)?;
        Ok(events)
    }

    /// 检查音轨缓存是否存在
    pub fn has_track(&self, track_index: usize) -> bool {
        self.track_path(track_index).exists()
    }

    fn track_path(&self, track_index: usize) -> PathBuf {
        self.cache_dir
            .join(format!("track_{:04x}.zst", track_index))
    }

    /// 清理此源文件对应的缓存
    pub fn cleanup(&self) -> std::io::Result<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

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
        let disk_cache = DiskTrackCache::new(cache_base_dir, source_path)
            .map_err(|e| format!("创建磁盘缓存失败: {e}"))?;

        // ═══════ 读取文件到内存 (mmap) ═══════
        let file = File::open(source_path).map_err(|e| format!("打开文件失败: {e}"))?;

        // SAFETY: 内存映射操作本身是安全的，但需要确保：
        // 1. 文件句柄在 mmap 生命周期内保持有效（由 file 变量持有）
        // 2. 文件内容在 mmap 生命周期内不被修改（由操作系统保证）
        // 3. 内存映射区域用于只读访问（midly::parse 只读取数据）
        // 4. mmap 在此作用域内有效，解析完成后立即释放
        let mmap =
            unsafe { memmap2::Mmap::map(&file).map_err(|e| format!("内存映射失败: {e}"))? };

        // ═══════ 使用 midly::parse() 获取懒 TrackIter ═══════
        let (header, track_iter) =
            midly::parse(&mmap[..]).map_err(|e| format!("解析 MIDI 头部失败: {e}"))?;

        let division = match header.timing {
            midly::Timing::Metrical(ticks) => ticks.as_int(),
            _ => DEFAULT_PPQN,
        };

        // 先收集所有 track 的 EventIter（零拷贝切片引用，不解析事件）
        let event_iters: Vec<_> = track_iter
            .collect::<midly::Result<Vec<_>>>()
            .map_err(|e| format!("解析音轨块失败: {e}"))?;

        let track_count = event_iters.len();

        if let Some(cb) = progress_callback {
            cb(PROGRESS_START);
        }

        // ═══════ 启动后台磁盘写入线程 ═══════
        let (disk_tx, disk_rx) = mpsc::channel::<(usize, Vec<MidiEvent>)>();
        let disk_cache_dir = disk_cache.cache_dir().to_path_buf();

        let disk_writer = std::thread::spawn(move || -> Result<(), String> {
            for (track_idx, events) in disk_rx {
                let track_path = disk_cache_dir.join(format!("track_{:04x}.zst", track_idx));
                let serialized =
                    bincode::serialize(&events).map_err(|e| format!("序列化失败: {e}"))?;
                let compressed = zstd::stream::encode_all(&mut &serialized[..], COMPRESSION_LEVEL)
                    .map_err(|e| format!("压缩失败: {e}"))?;
                let mut file_out =
                    File::create(&track_path).map_err(|e| format!("创建缓存文件失败: {e}"))?;
                file_out
                    .write_all(&compressed)
                    .map_err(|e| format!("写入缓存失败: {e}"))?;
            }
            Ok(())
        });

        // ═══════ 逐个音轨解析事件，边统计边分配 ═══════
        let memory_limit = max_ram_bytes.unwrap_or(MEMORY_LIMIT_BYTES);
        let loaded_memory_limit = memory_limit / 4;
        let initial_memory_limit = memory_limit - loaded_memory_limit;
        let mut memory_used: usize = 0;

        let mut in_memory_tracks: HashMap<usize, Vec<MidiEvent>> = HashMap::new();
        let mut summaries: Vec<TrackSummary> = Vec::with_capacity(track_count);

        for (track_idx, event_iter) in event_iters.into_iter().enumerate() {
            // 逐事件解析这一个音轨（event_iter 是 EventIter，只引用 mmap 中的切片）
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
                    Self::parse_track_event(track_idx, current_tick, &track_event.kind)
                {
                    // 统计
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

            let event_count = events.len() as u64;
            let should_try_memory = high_vel_count > 0;

            if should_try_memory {
                let track_size = estimate_events_size(&events);

                if memory_used + track_size <= initial_memory_limit {
                    memory_used += track_size;
                    summaries.push(TrackSummary {
                        track_index: track_idx,
                        event_count,
                        note_count,
                        high_vel_note_count: high_vel_count,
                        max_tick,
                        memory_bytes: track_size,
                        location: TrackLocationSerde::InMemory,
                    });
                    // 如果存在高力度音符，则完整加载到内存
                    in_memory_tracks.insert(track_idx, events.clone());

                    // 同样写入磁盘以备不时之需（或用于编辑）
                    disk_tx
                        .send((track_idx, events))
                        .map_err(|e| format!("发送磁盘写入任务失败: {e}"))?;
                } else {
                    summaries.push(TrackSummary {
                        track_index: track_idx,
                        event_count,
                        note_count,
                        high_vel_note_count: high_vel_count,
                        max_tick,
                        memory_bytes: 0,
                        location: TrackLocationSerde::OnDisk,
                    });
                    disk_tx
                        .send((track_idx, events))
                        .map_err(|e| format!("发送磁盘写入任务失败: {e}"))?;
                }
            } else {
                summaries.push(TrackSummary {
                    track_index: track_idx,
                    event_count,
                    note_count,
                    high_vel_note_count: high_vel_count,
                    max_tick,
                    memory_bytes: 0,
                    location: TrackLocationSerde::OnDisk,
                });
                disk_tx
                    .send((track_idx, events))
                    .map_err(|e| format!("发送磁盘写入任务失败: {e}"))?;
            }

            if let Some(cb) = progress_callback {
                let progress = PROGRESS_START
                    + PROGRESS_MAIN_RATIO * ((track_idx + 1) as f64 / track_count as f64);
                cb(progress);
            }
        }

        // 释放 mmap
        drop(mmap);
        drop(file);

        // 关闭 channel，等待后台写入完成
        drop(disk_tx);
        disk_writer
            .join()
            .map_err(|_| "磁盘写入线程 panic".to_string())?
            .map_err(|e| format!("磁盘写入失败: {e}"))?;

        if let Some(cb) = progress_callback {
            cb(1.0);
        }

        let in_mem_count = in_memory_tracks.len();
        let on_disk_count = track_count - in_mem_count;

        tracing::info!(
            "MidiMemoryManager: {} 音轨在内存 ({} MB), {} 音轨在磁盘, division={}",
            in_mem_count,
            memory_used / 1024 / 1024,
            on_disk_count,
            division,
        );

        Ok(Self {
            in_memory_tracks,
            loaded_tracks: HashMap::new(),
            track_summaries: summaries,
            disk_cache,
            memory_used: AtomicUsize::new(memory_used),
            memory_limit,
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
    fn parse_track_event(
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
            // 安全地获取引用，因为刚刚检查了 key 存在
            return Ok(self.in_memory_tracks.get(&track_index).unwrap());
        }

        // 检查是否已经从磁盘加载
        if self.loaded_tracks.contains_key(&track_index) {
            // 更新 LRU
            self.touch_lru(track_index);
            return Ok(self.loaded_tracks.get(&track_index).unwrap());
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

        Ok(self.loaded_tracks.get(&track_index).unwrap())
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
                self.lru_order.remove(0);
            }
            // 如果 remove 返回 None，说明 tracks 中没有这个条目，但 lru_order 中有
            // 这种情况下也应该从 lru_order 中移除，以保持一致性
            else {
                self.lru_order.remove(0);
            }
        }
    }
}

/// 管理器统计信息
#[derive(Debug, Clone)]
pub struct ManagerStats {
    pub track_count: usize,
    pub in_memory_track_count: usize,
    pub on_disk_track_count: usize,
    pub loaded_track_count: usize,
    pub base_memory_bytes: usize,
    pub loaded_memory_bytes: usize,
    pub total_memory_bytes: usize,
    pub memory_limit_bytes: usize,
    pub total_notes: u64,
    pub high_velocity_notes: u64,
}

impl std::fmt::Display for ManagerStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "MIDI 内存管理器统计:")?;
        writeln!(f, "  总音轨: {}", self.track_count)?;
        writeln!(f, "  内存音轨: {}", self.in_memory_track_count)?;
        writeln!(f, "  磁盘音轨: {}", self.on_disk_track_count)?;
        writeln!(f, "  按需加载音轨: {}", self.loaded_track_count)?;
        writeln!(
            f,
            "  基础内存: {:.2} MB",
            self.base_memory_bytes as f64 / 1024.0 / 1024.0
        )?;
        writeln!(
            f,
            "  按需内存: {:.2} MB",
            self.loaded_memory_bytes as f64 / 1024.0 / 1024.0
        )?;
        writeln!(
            f,
            "  总内存: {:.2} MB / {:.2} MB",
            self.total_memory_bytes as f64 / 1024.0 / 1024.0,
            self.memory_limit_bytes as f64 / 1024.0 / 1024.0
        )?;
        writeln!(f, "  总音符: {}", self.total_notes)?;
        writeln!(f, "  高力度音符(>1): {}", self.high_velocity_notes)?;
        Ok(())
    }
}

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
