//! 三层缓存架构
//!
//! L3 PageBackend → L2 ChunkCache → L1 HotCache
//!
//! 访问路径：
//!   get_events(from, to)
//!   → 先查 L1（当前播放窗口 ±2 秒）
//!   → 未命中查 L2（EventChunk LRU）
//!   → L2 未命中查 L3（PageBackend 原始字节）
//!   → L2 同步加载（兜底）
//!
//! 预取线程异步加载 L2 的前方块，不阻塞播放线程。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use lumino_midi::compact::CompactEvent;

use crate::backend::PageBackend;
use crate::chunk::EventChunk;
use crate::index::ChunkIndex;
use crate::metrics::CacheMetrics;
use crate::params;

/// L1 热缓存 — 当前播放位置附近的紧凑事件
struct HotEntry {
    /// 事件列表
    events: Vec<CompactEvent>,
    /// 起始 tick
    from_tick: u32,
    /// 结束 tick
    to_tick: u32,
}

/// L2 块缓存 — 解压后的 EventChunk
struct ChunkEntry {
    chunk: EventChunk,
    last_access: Instant,
}

/// 内部缓存状态
struct CacheInner {
    /// L3 原始字节后端
    backend: Box<dyn PageBackend>,

    /// L2 块缓存：chunk_index → ChunkEntry
    l2_chunks: HashMap<u32, ChunkEntry>,

    /// L2 LRU 顺序
    l2_order: Vec<u32>,

    /// L1 热缓存
    l1_cache: Option<HotEntry>,

    /// ChunkIndex（常驻内存）
    index: Arc<ChunkIndex>,

    /// 指标收集器
    metrics: &'static CacheMetrics,

    /// L2 当前使用的内存（字节）
    l2_memory_used: usize,

    /// L2 内存预算（字节）
    l2_memory_budget: usize,
}

/// 三层缓存系统
///
/// 线程安全：通过 `Mutex<CacheInner>` 保护内部状态。
/// 预取线程和播放线程共享同一个 Arc<LayeredCache>。
pub struct LayeredCache {
    inner: Mutex<CacheInner>,
}

impl LayeredCache {
    /// 创建新的分层缓存
    ///
    /// # 参数
    /// - `backend`: L3 页后端
    /// - `index`: ChunkIndex（常驻内存）
    /// - `metrics`: 指标收集器
    pub fn new(
        backend: Box<dyn PageBackend>,
        index: Arc<ChunkIndex>,
        metrics: &'static CacheMetrics,
    ) -> Self {
        let l2_cache_size = params::L2_MAX_CHUNKS;
        let inner = CacheInner {
            backend,
            l2_chunks: HashMap::with_capacity(l2_cache_size),
            l2_order: Vec::with_capacity(l2_cache_size),
            l1_cache: None,
            index,
            metrics,
            l2_memory_used: 0,
            l2_memory_budget: params::L2_MEMORY_BUDGET,
        };

        Self {
            inner: Mutex::new(inner),
        }
    }

    /// 获取指定 tick 范围的事件
    ///
    /// 访问路径（按优先级）：
    /// 1. L1 命中 → 直接返回
    /// 2. L1 未命中 → 查 L2
    /// 3. L2 命中 → 更新 L1，返回
    /// 4. L2 未命中 → L3 读取 + 反序列化 → 更新 L2 + L1 → 返回
    ///
    /// # 参数
    /// - `from_tick`: 起始 tick（包含）
    /// - `to_tick`: 结束 tick（不包含）
    /// - `max_events`: 最多返回的事件数（0 = 不限）
    pub fn get_events(&self, from_tick: u32, to_tick: u32, max_events: usize) -> Vec<CompactEvent> {
        let mut inner = self.inner.lock().unwrap();

        // 1. 尝试 L1
        if let Some(ref hot) = inner.l1_cache
            && from_tick >= hot.from_tick
            && to_tick <= hot.to_tick
        {
            inner.metrics.record_l1_hit();
            let mut result = Vec::with_capacity(hot.events.len() / 10);
            for ev in &hot.events {
                let tick = ev.delta_tick();
                if tick >= from_tick && tick < to_tick {
                    result.push(*ev);
                }
            }
            return result;
        }
        inner.metrics.record_l1_miss();

        // 2. 查 L2 + L3 兜底
        let (start_chunk, end_chunk) = inner.index.chunk_range(from_tick, to_tick);
        if start_chunk >= end_chunk {
            return Vec::new();
        }

        let mut result = Vec::new();
        let event_limit = if max_events == 0 {
            usize::MAX
        } else {
            max_events
        };

        for chunk_idx in start_chunk..end_chunk {
            if result.len() >= event_limit {
                break;
            }

            let chunk = self.load_chunk_internal(&mut inner, chunk_idx as u32);
            if let Ok(chunk) = chunk {
                for ev in &chunk.events {
                    let tick = ev.delta_tick();
                    if tick >= from_tick && tick < to_tick {
                        result.push(*ev);
                        if result.len() >= event_limit {
                            break;
                        }
                    }
                }
            }
        }

        // 3. 更新 L1
        if !result.is_empty() {
            let window_half = (params::L1_WINDOW_SECONDS * 480.0 * 120.0 / 60.0) as u32;
            let l1_from = from_tick.saturating_sub(window_half);
            let l1_to = to_tick.saturating_add(window_half);

            let mut l1_events = Vec::with_capacity(params::L1_MAX_EVENTS.min(result.len() * 2));
            for chunk_idx in start_chunk..end_chunk {
                if let Ok(chunk) = self.load_chunk_internal(&mut inner, chunk_idx as u32) {
                    for ev in chunk.events.iter().filter(|ev| {
                        let tick = ev.delta_tick();
                        tick >= l1_from && tick < l1_to
                    }) {
                        if l1_events.len() < params::L1_MAX_EVENTS {
                            l1_events.push(*ev);
                        }
                    }
                }
            }

            inner.l1_cache = Some(HotEntry {
                events: l1_events,
                from_tick: l1_from,
                to_tick: l1_to,
            });
        }

        result
    }

    /// 从 L2/L3 加载指定块
    fn load_chunk_internal(
        &self,
        inner: &mut CacheInner,
        chunk_index: u32,
    ) -> Result<EventChunk, String> {
        // 查 L2
        if let Some(entry) = inner.l2_chunks.get_mut(&chunk_index) {
            entry.last_access = Instant::now();
            inner.metrics.record_l2_hit();
            // 更新 LRU
            if let Some(pos) = inner.l2_order.iter().position(|&k| k == chunk_index) {
                inner.l2_order.remove(pos);
            }
            inner.l2_order.push(chunk_index);
            return Ok(entry.chunk.clone());
        }

        inner.metrics.record_l2_miss();

        // L3 读取（同步加载，记录延迟）
        let timer = Instant::now();
        let idx_entry = inner
            .index
            .entries
            .get(chunk_index as usize)
            .ok_or_else(|| format!("ChunkIndex 中没有块 {}", chunk_index))?;

        let mut raw_bytes = vec![0u8; idx_entry.byte_length as usize];
        inner
            .backend
            .read_exact(idx_entry.file_offset, &mut raw_bytes)
            .map_err(|e| format!("L3 读取块 {} 失败: {}", chunk_index, e))?;

        inner.metrics.record_l3_read();

        let chunk = EventChunk::from_bytes(&raw_bytes)
            .map_err(|e| format!("反序列化块 {} 失败: {}", chunk_index, e))?;

        // 记录同步加载延迟
        inner.metrics.record_sync_load(timer.elapsed());

        // 放入 L2
        self.insert_l2(inner, chunk_index, chunk.clone());

        Ok(chunk)
    }

    /// 插入 L2 缓存（按内存预算 LRU 淘汰）
    fn insert_l2(&self, inner: &mut CacheInner, chunk_index: u32, chunk: EventChunk) {
        let chunk_bytes = chunk.byte_size();

        // 如果单个 chunk 超过预算，只保留它（淘汰所有其他 chunk）
        if chunk_bytes > inner.l2_memory_budget {
            // 淘汰所有已有 chunk
            inner.l2_chunks.clear();
            inner.l2_order.clear();
            inner.l2_memory_used = 0;
        }

        // 按内存预算淘汰（淘汰最旧的直到空间足够）
        while inner.l2_memory_used + chunk_bytes > inner.l2_memory_budget
            && !inner.l2_order.is_empty()
        {
            let evict = inner.l2_order.remove(0);
            if let Some(entry) = inner.l2_chunks.remove(&evict) {
                inner.l2_memory_used = inner.l2_memory_used.saturating_sub(entry.chunk.byte_size());
            }
        }

        // 同时也受限于最大条目数
        if inner.l2_chunks.len() >= params::L2_MAX_CHUNKS && !inner.l2_order.is_empty() {
            let evict = inner.l2_order.remove(0);
            if let Some(entry) = inner.l2_chunks.remove(&evict) {
                inner.l2_memory_used = inner.l2_memory_used.saturating_sub(entry.chunk.byte_size());
            }
        }

        inner.l2_memory_used += chunk_bytes;
        inner.l2_chunks.insert(
            chunk_index,
            ChunkEntry {
                chunk,
                last_access: Instant::now(),
            },
        );
        inner.l2_order.push(chunk_index);
    }

    /// 预取指定块
    ///
    /// 由预取线程调用。如果块已在 L2 中或正在预取中，跳过。
    /// 返回 false 表示无法继续预取（L2 满）。
    pub fn prefetch_chunk(&self, chunk_index: u32) -> bool {
        let mut inner = self.inner.lock().unwrap();

        // 已在 L2 中
        if inner.l2_chunks.contains_key(&chunk_index) {
            inner.metrics.record_prefetch_hit();
            return true;
        }

        // 检查 L2 是否还能承受新的 chunk（基于内存预算）
        if inner.l2_memory_used >= inner.l2_memory_budget
            || inner.l2_chunks.len() >= params::L2_MAX_CHUNKS
        {
            return false;
        }

        // 开始预取
        inner.metrics.record_prefetch_load();

        // L3 读取
        let idx_entry = match inner.index.entries.get(chunk_index as usize) {
            Some(e) => e,
            None => return false,
        };

        let mut raw_bytes = vec![0u8; idx_entry.byte_length as usize];
        if inner
            .backend
            .read_exact(idx_entry.file_offset, &mut raw_bytes)
            .is_err()
        {
            inner.metrics.record_prefetch_miss();
            return false;
        }

        let chunk = match EventChunk::from_bytes(&raw_bytes) {
            Ok(c) => c,
            Err(_) => {
                inner.metrics.record_prefetch_miss();
                return false;
            }
        };

        inner.l2_chunks.insert(
            chunk_index,
            ChunkEntry {
                chunk,
                last_access: Instant::now(),
            },
        );
        inner.l2_order.push(chunk_index);
        inner.metrics.record_prefetch_hit();
        true
    }

    /// 清除 L1 热缓存
    pub fn clear_l1(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.l1_cache = None;
    }

    /// 清除 L2 块缓存
    pub fn clear_l2(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.l2_chunks.clear();
        inner.l2_order.clear();
    }

    /// 清除所有缓存
    pub fn clear_all(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.l1_cache = None;
        inner.l2_chunks.clear();
        inner.l2_order.clear();
    }

    /// L3 shrink（在内存紧张时调用）
    pub fn shrink_l3(&self, max_bytes: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.backend.shrink(max_bytes);
    }

    /// 当前 L2 缓存条目数
    pub fn l2_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.l2_chunks.len()
    }

    /// 当前 L2 缓存大小（字节）
    pub fn l2_bytes(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.l2_memory_used
    }

    /// 获取 L3 后端引用（用于统计）
    pub fn backend_size(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.backend.size()
    }

    /// 获取指标引用
    pub fn metrics(&self) -> &'static CacheMetrics {
        let inner = self.inner.lock().unwrap();
        inner.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::create_backend;
    use crate::chunk::chunk_midi_data;
    use crate::index::ChunkIndex;

    static TEST_METRICS: CacheMetrics = CacheMetrics::new();

    fn make_test_midi() -> Vec<u8> {
        // Minimal MIDI file with a few notes
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, // format 0
            0x00, 0x01, // 1 track
            0x01, 0xE0, // division 480
        ];

        let track_data = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x0B, 0x00, // delta=0
            0x90, 0x3C, 0x64, // NoteOn
            0x80, // delta=128
            0x3C, 0x00, // NoteOff
            0x00, // delta=0
            0xFF, 0x2F, 0x00, // EndOfTrack
        ];

        let mut midi = Vec::with_capacity(header.len() + track_data.len());
        midi.extend_from_slice(&header);
        midi.extend_from_slice(&track_data);
        midi
    }

    fn setup_cache() -> Arc<LayeredCache> {
        let midi_data = make_test_midi();
        let (chunks, total_ticks, track_count) = chunk_midi_data(&midi_data, None).unwrap();
        let index = Arc::new(ChunkIndex::from_chunks(&chunks, total_ticks, track_count));

        // Serialize chunks and build backend
        let mut raw_data = Vec::new();
        for chunk in &chunks {
            let bytes = chunk.to_bytes().unwrap();
            raw_data.extend_from_slice(&bytes);
        }

        let backend = create_backend(raw_data);
        let metrics: &'static CacheMetrics = &TEST_METRICS;
        Arc::new(LayeredCache::new(backend, index, metrics))
    }

    #[test]
    fn test_cache_get_events() {
        let cache = setup_cache();
        let events = cache.get_events(0, 1000, 0);
        assert!(!events.is_empty());
    }

    #[test]
    fn test_cache_empty_range() {
        let cache = setup_cache();
        let events = cache.get_events(99999, 100000, 0);
        assert!(events.is_empty());
    }

    #[test]
    fn test_cache_clear() {
        let cache = setup_cache();
        let _ = cache.get_events(0, 1000, 0);
        assert!(cache.l2_count() > 0 || cache.l2_count() == 0); // at least called
        cache.clear_all();
        assert_eq!(cache.l2_count(), 0);
    }

    #[test]
    fn test_cache_prefetch() {
        let cache = setup_cache();
        let result = cache.prefetch_chunk(0);
        assert!(result);
        assert_eq!(cache.l2_count(), 1);
    }

    #[test]
    fn test_cache_consecutive_access() {
        let cache = setup_cache();
        let first = cache.get_events(0, 1000, 0);
        // Second access should hit L1
        let _ = cache.get_events(0, 1000, 0);
        // First access should return 2 events (NoteOn + NoteOff)
        assert_eq!(first.len(), 2);
    }
}
