//! 三层缓存架构
//!
//! L3 PageBackend → L2 (raw bytes) → L1 (CompactEvent)
//!
//! 关键优化：L2 miss 时不读整个 chunk，而是：
//! 1. 读 44 字节 header
//! 2. 在文件上二分查找目标 tick（每次 4 字节读取，~25 次 seek）
//! 3. 只读 [start_idx, start_idx+max_events) 范围的事件数据

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use lumino_midi::compact::CompactEvent;

use crate::backend::PageBackend;
use crate::chunk::{CHUNK_HEADER_SIZE, EventChunk};
use crate::index::ChunkIndex;
use crate::metrics::CacheMetrics;
use crate::params;

struct HotEntry {
    events: Vec<CompactEvent>,
    from_tick: u32,
    to_tick: u32,
}
struct ChunkEntry {
    raw_data: Vec<u8>,
}

struct CacheInner {
    backend: Box<dyn PageBackend>,
    l2_chunks: HashMap<u32, ChunkEntry>,
    l2_order: Vec<u32>,
    l1_cache: Option<HotEntry>,
    index: Arc<ChunkIndex>,
    metrics: &'static CacheMetrics,
    l2_memory_used: usize,
    l2_memory_budget: usize,
}

pub struct LayeredCache {
    inner: Mutex<CacheInner>,
}

impl LayeredCache {
    pub fn new(
        backend: Box<dyn PageBackend>,
        index: Arc<ChunkIndex>,
        metrics: &'static CacheMetrics,
    ) -> Self {
        let cap = params::L2_MAX_CHUNKS;
        Self {
            inner: Mutex::new(CacheInner {
                backend,
                l2_chunks: HashMap::with_capacity(cap),
                l2_order: Vec::with_capacity(cap),
                l1_cache: None,
                index,
                metrics,
                l2_memory_used: 0,
                l2_memory_budget: params::L2_MEMORY_BUDGET,
            }),
        }
    }

    pub fn get_events(&self, from_tick: u32, to_tick: u32, max_events: usize) -> Vec<CompactEvent> {
        let mut inner = self.inner.lock().unwrap();
        let event_limit = if max_events == 0 {
            usize::MAX
        } else {
            max_events
        };

        // 1. L1
        if let Some(ref hot) = inner.l1_cache
            && from_tick >= hot.from_tick
            && to_tick <= hot.to_tick
        {
            inner.metrics.record_l1_hit();
            return Self::filter_events(&hot.events, from_tick, to_tick, event_limit);
        }
        inner.metrics.record_l1_miss();

        let (start_chunk, end_chunk) = inner.index.chunk_range(from_tick, to_tick);
        if start_chunk >= end_chunk {
            return Vec::new();
        }

        let mut result = Vec::new();
        for chunk_idx in start_chunk..end_chunk {
            if result.len() >= event_limit {
                break;
            }

            let remaining = event_limit.saturating_sub(result.len());

            // L2 hit → binary search in cached raw bytes
            if let Some(entry) = inner.l2_chunks.get(&(chunk_idx as u32)) {
                inner.metrics.record_l2_hit();
                let (events, _) = EventChunk::read_events_in_range(
                    &entry.raw_data,
                    from_tick,
                    to_tick,
                    remaining,
                );
                result.extend(events);
                continue;
            }
            inner.metrics.record_l2_miss();

            // L2 miss → partial file read (DO NOT load entire chunk)
            let idx_entry = match inner.index.entries.get(chunk_idx) {
                Some(e) => e,
                None => continue,
            };

            let timer = Instant::now();

            match Self::read_chunk_partial(
                &*inner.backend,
                idx_entry.file_offset,
                idx_entry.byte_length as usize,
                from_tick,
                to_tick,
                remaining,
            ) {
                Ok(events) => {
                    inner.metrics.record_l3_read();
                    inner.metrics.record_sync_load(timer.elapsed());
                    result.extend(events);
                }
                Err(_) => continue,
            }
        }

        // 3. Warm L1
        if !result.is_empty() {
            let window = (params::L1_WINDOW_SECONDS * 480.0 * 120.0 / 60.0) as u32;
            let l1_from = from_tick.saturating_sub(window);
            let l1_to = to_tick.saturating_add(window);
            let mut l1_events = Vec::with_capacity(params::L1_MAX_EVENTS);
            for chunk_idx in start_chunk..end_chunk {
                if l1_events.len() >= params::L1_MAX_EVENTS {
                    break;
                }
                if let Some(entry) = inner.l2_chunks.get(&(chunk_idx as u32)) {
                    let (events, _) = EventChunk::read_events_in_range(
                        &entry.raw_data,
                        l1_from,
                        l1_to,
                        params::L1_MAX_EVENTS.saturating_sub(l1_events.len()),
                    );
                    l1_events.extend(events);
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

    /// 部分读取 chunk：仅读 header + 二分查找 + 读目标范围事件
    fn read_chunk_partial(
        backend: &dyn PageBackend,
        file_offset: u64,
        byte_length: usize,
        from_tick: u32,
        to_tick: u32,
        max_events: usize,
    ) -> Result<Vec<CompactEvent>, String> {
        if byte_length < CHUNK_HEADER_SIZE {
            return Err("chunk too small".into());
        }

        // 读 44 字节 header
        let mut header = [0u8; CHUNK_HEADER_SIZE];
        backend
            .read_exact(file_offset, &mut header)
            .map_err(|e| format!("读 header 失败: {e}"))?;

        let event_count =
            u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
        if event_count == 0 {
            return Ok(Vec::new());
        }

        let data_offset = file_offset + CHUNK_HEADER_SIZE as u64;
        let _actual_event_bytes = event_count * 12;

        // 文件上二分查找起始 tick
        let start_idx = Self::binary_search_file(backend, data_offset, event_count, from_tick)?;
        if start_idx >= event_count {
            return Ok(Vec::new());
        }

        // 读目标范围的事件数据
        let read_count = (event_count - start_idx).min(max_events);
        let read_start_offset = data_offset + (start_idx * 12) as u64;
        let read_len = read_count * 12;

        let mut event_data = vec![0u8; read_len];
        backend
            .read_exact(read_start_offset, &mut event_data)
            .map_err(|e| format!("读事件数据失败: {e}"))?;

        // 解析个 CompactEvent
        let mut result = Vec::with_capacity(read_count);
        for i in 0..read_count {
            let off = i * 12;
            let ev = CompactEvent::from_bytes(unsafe {
                &*(event_data[off..off + 12].as_ptr() as *const [u8; 12])
            });
            if ev.delta_tick() >= to_tick {
                break;
            }
            result.push(ev);
        }

        Ok(result)
    }

    /// 在文件中的事件数据上二分查找
    fn binary_search_file(
        backend: &dyn PageBackend,
        data_offset: u64,
        event_count: usize,
        target_tick: u32,
    ) -> Result<usize, String> {
        let mut low = 0usize;
        let mut high = event_count;
        while low < high {
            let mid = low + (high - low) / 2;
            let tick_off = data_offset + (mid * 12) as u64;
            let mut buf = [0u8; 4];
            backend
                .read_exact(tick_off, &mut buf)
                .map_err(|e| format!("二分查找读失败: {e}"))?;
            let tick = u32::from_le_bytes(buf);
            if tick < target_tick {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Ok(low)
    }

    fn filter_events(
        events: &[CompactEvent],
        from: u32,
        to: u32,
        limit: usize,
    ) -> Vec<CompactEvent> {
        let mut result = Vec::new();
        for ev in events {
            let t = ev.delta_tick();
            if t >= from && t < to {
                result.push(*ev);
                if result.len() >= limit {
                    break;
                }
            }
        }
        result
    }

    /// 全量加载 chunk 到 L2（由预取线程调用）
    fn load_chunk_full(&self, inner: &mut CacheInner, chunk_index: u32) -> Result<Vec<u8>, String> {
        let idx_entry = inner
            .index
            .entries
            .get(chunk_index as usize)
            .ok_or_else(|| format!("ChunkIndex 无块 {}", chunk_index))?;
        let mut raw = vec![0u8; idx_entry.byte_length as usize];
        inner
            .backend
            .read_exact(idx_entry.file_offset, &mut raw)
            .map_err(|e| format!("L3 读块 {} 失败: {}", chunk_index, e))?;
        inner.metrics.record_l3_read();
        self.insert_l2_raw(inner, chunk_index, raw.clone());
        Ok(raw)
    }

    fn insert_l2_raw(&self, inner: &mut CacheInner, chunk_index: u32, raw_data: Vec<u8>) {
        let size = raw_data.len();
        if size > inner.l2_memory_budget {
            inner.l2_chunks.clear();
            inner.l2_order.clear();
            inner.l2_memory_used = 0;
        }
        while inner.l2_memory_used + size > inner.l2_memory_budget && !inner.l2_order.is_empty() {
            let evict = inner.l2_order.remove(0);
            if let Some(e) = inner.l2_chunks.remove(&evict) {
                inner.l2_memory_used = inner.l2_memory_used.saturating_sub(e.raw_data.len());
            }
        }
        if inner.l2_chunks.len() >= params::L2_MAX_CHUNKS && !inner.l2_order.is_empty() {
            let evict = inner.l2_order.remove(0);
            if let Some(e) = inner.l2_chunks.remove(&evict) {
                inner.l2_memory_used = inner.l2_memory_used.saturating_sub(e.raw_data.len());
            }
        }
        inner.l2_memory_used += size;
        inner.l2_chunks.insert(chunk_index, ChunkEntry { raw_data });
        inner.l2_order.push(chunk_index);
    }

    pub fn prefetch_chunk(&self, chunk_index: u32) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.l2_chunks.contains_key(&chunk_index) {
            inner.metrics.record_prefetch_hit();
            return true;
        }
        if inner.l2_memory_used >= inner.l2_memory_budget
            || inner.l2_chunks.len() >= params::L2_MAX_CHUNKS
        {
            return false;
        }
        inner.metrics.record_prefetch_load();
        self.load_chunk_full(&mut inner, chunk_index).is_ok()
    }

    pub fn clear_l1(&self) {
        self.inner.lock().unwrap().l1_cache = None;
    }
    pub fn clear_l2(&self) {
        let mut i = self.inner.lock().unwrap();
        i.l2_chunks.clear();
        i.l2_order.clear();
        i.l2_memory_used = 0;
    }
    pub fn clear_all(&self) {
        self.clear_l1();
        self.clear_l2();
    }
    pub fn shrink_l3(&self, max_bytes: u64) {
        self.inner.lock().unwrap().backend.shrink(max_bytes);
    }
    pub fn l2_count(&self) -> usize {
        self.inner.lock().unwrap().l2_chunks.len()
    }
    pub fn l2_bytes(&self) -> usize {
        self.inner.lock().unwrap().l2_memory_used
    }
    pub fn backend_size(&self) -> u64 {
        self.inner.lock().unwrap().backend.size()
    }
    pub fn metrics(&self) -> &'static CacheMetrics {
        self.inner.lock().unwrap().metrics
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
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track_data = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x90, 0x3C, 0x64, 0x80, 0x3C,
            0x00, 0x00, 0xFF, 0x2F, 0x00,
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
        let mut raw_data = Vec::new();
        for chunk in &chunks {
            raw_data.extend_from_slice(&chunk.to_raw_bytes());
        }
        let backend = create_backend(raw_data);
        Arc::new(LayeredCache::new(backend, index, &TEST_METRICS))
    }

    #[test]
    fn test_cache_get_events() {
        let c = setup_cache();
        assert!(!c.get_events(0, 1000, 0).is_empty());
    }
    #[test]
    fn test_cache_empty_range() {
        let c = setup_cache();
        assert!(c.get_events(99999, 100000, 0).is_empty());
    }
    #[test]
    fn test_cache_clear() {
        let c = setup_cache();
        c.clear_all();
        assert_eq!(c.l2_count(), 0);
    }
    #[test]
    fn test_cache_prefetch() {
        let c = setup_cache();
        assert!(c.prefetch_chunk(0));
        assert_eq!(c.l2_count(), 1);
    }
    #[test]
    fn test_cache_consecutive_access() {
        let c = setup_cache();
        let first = c.get_events(0, 1000, 0);
        let _ = c.get_events(0, 1000, 0);
        assert_eq!(first.len(), 2);
    }
}
