//! # lumino-cache — MIDI 数据分层缓存系统
//!
//! 专为黑乐谱（black MIDI）超大文件设计的缓存系统。
//! 支持 1GB+ 的 .mid 文件，内存可控在 1GB 以内。

pub mod backend;
pub mod cache;
pub mod chunk;
pub mod index;
pub mod metrics;
pub mod params;
pub mod prefetch;
pub mod track;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumino_midi::compact::CompactEvent;

pub use backend::{FileBackend, PageBackend, create_backend, create_file_backend};
pub use cache::LayeredCache;
pub use chunk::{
    ChunkIndexRawEntry, EventChunk, build_chunks_from_notes, chunk_midi_data,
    chunk_midi_data_streaming, chunk_midi_data_streaming_to_path, phase1_bucketize,
    phase2_assemble, phase2_assemble_to_path,
};
pub use index::{ChunkIndex, ChunkIndexEntry};
pub use metrics::CacheMetrics;
pub use params::CHUNK_TICK_SPAN;
pub use prefetch::{PrefetchHandle, spawn_prefetch_thread};
pub use track::{TrackDataRef, TrackManager, TrackView, TrackVisibility};

use thiserror::Error;

/// lumino-cache 错误类型
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("MIDI 解析失败: {0}")]
    MidiParse(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("反序列化错误: {0}")]
    Deserialize(String),
    #[error("块 {0} 不存在")]
    ChunkNotFound(u32),
}

pub type Result<T> = std::result::Result<T, CacheError>;

/// 缓存系统完整实例
pub struct MidiCache {
    pub cache: Arc<LayeredCache>,
    pub index: Arc<ChunkIndex>,
    pub prefetch: Option<PrefetchHandle>,
    pub tracks: TrackManager,
    pub metrics: &'static CacheMetrics,
    /// 后端临时文件路径（Drop 时清理）
    _tmp_chunk_path: Option<PathBuf>,
}

impl std::fmt::Debug for MidiCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiCache")
            .field("track_count", &self.index.track_count)
            .field("total_ticks", &self.index.total_ticks)
            .field("chunk_count", &self.index.len())
            .finish()
    }
}

impl MidiCache {
    /// 初始化缓存系统（通过 mmap + 桶排序的两遍方案）
    ///
    /// 流式解析 → 桶文件外部排序 → 直接写入磁盘文件 →
    /// FileBackend 按需读取（不保留全部数据在内存）。
    /// 仅 ChunkIndex（~160KB）常驻内存。
    ///
    /// 第 1 遍：解析 MIDI 并分发到 64 个桶文件
    /// 第 2 遍：按桶 O(N) 分组构建 Chunk → 序列化到输出文件
    /// 最终：输出文件由 FileBackend 管理，只按需读页
    pub fn load<P: AsRef<Path>>(
        midi_path: P,
        progress: Option<&'static dyn Fn(f64)>,
    ) -> Result<Self> {
        let path = midi_path.as_ref();

        // ── Phase 1：mmap 读 MIDI 文件，解析并分发到桶文件 ──
        // mmap 只在真正访问的页面上产生 RSS，加载过程中 RSS 更低。
        let file = std::fs::File::open(path).map_err(CacheError::Io)?;
        let file_data = unsafe { memmap2::Mmap::map(&file).map_err(CacheError::Io)? };
        drop(file); // 关闭 fd，mmap 保持引用

        let (tmp_dir, bucket_counters, track_count, total_ticks) =
            chunk::phase1_bucketize(&file_data, progress).map_err(CacheError::MidiParse)?;

        // ⭐ Phase 1 结束，立即释放 mmap（1.25GB 归还系统）
        drop(file_data);

        // ── Phase 2：从桶文件并行读取、构建 Chunk、直接写入输出文件 ──
        // 使用 phase2_assemble_to_path 直接写文件，无中间 copy
        let tmp_chunk_path = {
            let mut p = std::env::temp_dir();
            p.push(format!("lumino_cache_data_{:016x}", rand_temp()));
            p
        };
        let raw_entries =
            chunk::phase2_assemble_to_path(&tmp_dir, &bucket_counters, &tmp_chunk_path)
                .map_err(CacheError::MidiParse)?;

        // 构建 ChunkIndex（常驻内存）
        let index = Arc::new(ChunkIndex::from_raw_entries(
            raw_entries,
            total_ticks,
            track_count,
        ));

        // 创建文件后端（不读入内存，按需 file.seek + read）
        let backend = create_file_backend(&tmp_chunk_path)?;

        let metrics: &'static CacheMetrics = Box::leak(Box::new(CacheMetrics::new()));
        let cache = Arc::new(LayeredCache::new(backend, index.clone(), metrics));
        let prefetch = Some(spawn_prefetch_thread(cache.clone(), total_ticks));
        let tracks = TrackManager::new(track_count);

        Ok(Self {
            cache,
            index,
            prefetch,
            tracks,
            metrics,
            _tmp_chunk_path: Some(tmp_chunk_path),
        })
    }

    /// 使用 midly-fork 零分配 API 从 MIDI 文件直接构建缓存。
    ///
    /// 相比 `load()` 的两遍桶排序方案，此方法：
    /// - 使用 `extract_notes_from_bytes` 一次性提取所有音符，无 Smf 中间态
    /// - 直接按 tick 排序分组为块，不需要外部排序的桶文件
    /// - 峰值内存：~1.5GB（1GB 文件 + 500MB 音符）vs 原方案 5-8GB
    ///
    /// 适用于大文件（>100MB）的内存优化加载场景。
    pub fn from_notes_file<P: AsRef<Path>>(
        midi_path: P,
        progress: Option<&'static dyn Fn(f64)>,
    ) -> Result<Self> {
        let path = midi_path.as_ref();

        if let Some(cb) = progress {
            (cb)(0.05);
        }

        let file_bytes = std::fs::read(path).map_err(CacheError::Io)?;

        if let Some(cb) = progress {
            (cb)(0.15);
        }

        let (notes, tempo_changes) = midly::loader::extract_notes_from_bytes(&file_bytes)
            .map_err(|e| CacheError::MidiParse(format!("提取音符失败: {e}")))?;

        drop(file_bytes);

        if let Some(cb) = progress {
            (cb)(0.50);
        }

        let tmp_chunk_path = {
            let mut p = std::env::temp_dir();
            p.push(format!("lumino_cache_data_{:016x}", rand_temp()));
            p
        };

        let (raw_entries, total_ticks, track_count) =
            chunk::build_chunks_from_notes(&notes, &tempo_changes, &tmp_chunk_path)
                .map_err(CacheError::MidiParse)?;

        if let Some(cb) = progress {
            (cb)(0.90);
        }

        let index = Arc::new(ChunkIndex::from_raw_entries(
            raw_entries,
            total_ticks,
            track_count,
        ));

        let backend = create_file_backend(&tmp_chunk_path)?;

        let metrics: &'static CacheMetrics = Box::leak(Box::new(CacheMetrics::new()));
        let cache = Arc::new(LayeredCache::new(backend, index.clone(), metrics));
        let prefetch = Some(spawn_prefetch_thread(cache.clone(), total_ticks));
        let tracks = TrackManager::new(track_count);

        Ok(Self {
            cache,
            index,
            prefetch,
            tracks,
            metrics,
            _tmp_chunk_path: Some(tmp_chunk_path),
        })
    }

    /// 获取总 tick 数
    #[inline]
    pub fn total_ticks(&self) -> u32 {
        self.index.total_ticks
    }

    /// 获取指定音轨的所有事件（在 0..total_ticks 范围内）
    ///
    /// 注意：可能较慢，因为需要遍历所有块。
    /// 仅用于编辑器初始加载音轨的场景。
    pub fn get_track_events(&self, track_id: u16) -> Vec<CompactEvent> {
        let total = self.index.total_ticks;
        let mut events = self.cache.get_events(0, total, 0);
        events.retain(|ev| ev.track_id() == track_id);
        events
    }

    /// 获取指定音轨的所有音符（转换为 (tick, key, length, velocity) 格式）
    ///
    /// 从 CompactEvent NoteOn/NoteOff 对重构音符。
    /// 内存边界：最多返回 500_000 个音符（可配置）。
    pub fn get_track_notes(&self, track_id: u16) -> Vec<(f32, u8, f32, u8)> {
        use lumino_midi::compact::EventKind;
        use std::collections::HashMap;

        let events = self.get_track_events(track_id);
        let mut active_notes: HashMap<u8, (u32, u8)> = HashMap::new();
        let mut notes = Vec::new();

        for ev in &events {
            let tick = ev.delta_tick();
            let key = ev.param1() as u8;
            let vel = ev.param2() as u8;

            match ev.kind() {
                EventKind::NoteOn => {
                    if vel > 0 {
                        if let Some((start_tick, prev_vel)) = active_notes.remove(&key) {
                            let length = tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, key, length, prev_vel));
                        }
                        active_notes.insert(key, (tick, vel));
                    } else if let Some((start_tick, prev_vel)) = active_notes.remove(&key) {
                        let length = tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, key, length, prev_vel));
                    }
                }
                EventKind::NoteOff => {
                    if let Some((start_tick, prev_vel)) = active_notes.remove(&key) {
                        let length = tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, key, length, prev_vel));
                    }
                }
                _ => {}
            }
        }

        let last_tick = events.last().map(|e| e.delta_tick()).unwrap_or(0);
        for (key, (start_tick, vel)) in active_notes {
            let length = last_tick.saturating_sub(start_tick) as f32;
            notes.push((start_tick as f32, key, length, vel));
        }

        notes
    }

    /// 获取所有 tempo 变化事件
    pub fn get_tempo_changes(&self) -> Vec<(u32, f32)> {
        use lumino_midi::compact::EventKind;

        let total = self.index.total_ticks;
        let events = self.cache.get_events(0, total, 0);
        let mut tempos = Vec::new();

        for ev in &events {
            if ev.kind() == EventKind::Tempo {
                let tick = ev.delta_tick();
                let tempo_low = ev.param1() as u32;
                let tempo_high = ev.param2() as u32;
                let microseconds = (tempo_high << 16) | tempo_low;
                if microseconds > 0 {
                    let bpm = 60_000_000.0 / microseconds as f32;
                    tempos.push((tick, bpm));
                }
            }
        }

        if tempos.is_empty() {
            tempos.push((0, 120.0));
        }

        tempos
    }

    /// 获取音轨数量
    #[inline]
    pub fn track_count(&self) -> usize {
        self.index.track_count as usize
    }
}

impl Drop for MidiCache {
    fn drop(&mut self) {
        if let Some(ref path) = self._tmp_chunk_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn rand_temp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_nanos() as u64) ^ (now.as_nanos() >> 32) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_event_size() {
        use lumino_midi::compact::CompactEvent;
        assert_eq!(std::mem::size_of::<CompactEvent>(), 12);
    }

    #[test]
    fn test_chunk_tick_span() {
        assert_eq!(CHUNK_TICK_SPAN, 65536);
        assert!(CHUNK_TICK_SPAN.is_power_of_two());
    }
}
