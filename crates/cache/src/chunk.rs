//! 固定 tick 跨度的事件分块系统
//!
//! `EventChunk` 是缓存系统的基本数据单元。
//! 每个 chunk 覆盖 `CHUNK_TICK_SPAN` 个 tick（默认 65536）。
//!
//! 流式分块：使用 64 桶外部排序，避免将全部事件保留在内存中。
//! 内存峰值：O(1 个桶的数据) ≈ 总事件数 / 64。
//!
//! 内存占用预估：
//! - EventChunk 元数据: ~32 字节
//! - 每个 CompactEvent: 12 字节

use std::io::{SeekFrom, Write};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumino_midi::compact::{CompactEvent, EventKind};
use midly::{MetaMessage, MidiMessage, Timing, TrackEventKind};

use crate::params;

/// 外部排序的桶数量
const NUM_BUCKETS: u32 = 64;

/// 原始块头大小（字节）
pub const CHUNK_HEADER_SIZE: usize = 44;

/// 事件块 — 固定 tick 跨度的事件集合（反序列化表示）
#[derive(Debug, Clone)]
pub struct EventChunk {
    pub start_tick: u32,
    pub end_tick: u32,
    pub events: Vec<CompactEvent>,
    pub track_mask: [u64; 4],
}

impl EventChunk {
    pub fn new(start_tick: u32, events: Vec<CompactEvent>) -> Self {
        let end_tick = start_tick.saturating_add(params::CHUNK_TICK_SPAN);
        let mut track_mask = [0u64; 4];
        for ev in &events {
            let tid = ev.track_id() as usize;
            if tid < 256 {
                track_mask[tid / 64] |= 1u64 << (tid % 64);
            }
        }
        Self {
            start_tick,
            end_tick,
            events,
            track_mask,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn has_track(&self, track_id: u16) -> bool {
        let tid = track_id as usize;
        if tid >= 256 {
            return false;
        }
        (self.track_mask[tid / 64] & (1u64 << (tid % 64))) != 0
    }

    pub fn byte_size(&self) -> usize {
        mem::size_of::<Self>() + self.events.len() * mem::size_of::<CompactEvent>()
    }

    /// 序列化为原始二进制格式（44 字节头 + 12*N 字节事件）
    ///
    /// 格式：
    /// ```text
    /// [0..4)  start_tick: u32 LE
    /// [4..8)  end_tick: u32 LE
    /// [8..12) event_count: u32 LE
    /// [12..44) track_mask: [u64; 4] LE
    /// [44..)  events: CompactEvent × event_count (12 bytes each)
    /// ```
    pub fn to_raw_bytes(&self) -> Vec<u8> {
        let event_count = self.events.len() as u32;
        let mut buf = Vec::with_capacity(CHUNK_HEADER_SIZE + event_count as usize * 12);
        buf.extend_from_slice(&self.start_tick.to_le_bytes());
        buf.extend_from_slice(&self.end_tick.to_le_bytes());
        buf.extend_from_slice(&event_count.to_le_bytes());
        for &mask in &self.track_mask {
            buf.extend_from_slice(&mask.to_le_bytes());
        }
        for ev in &self.events {
            buf.extend_from_slice(ev.as_bytes());
        }
        buf
    }

    /// 从原始二进制反序列化
    pub fn from_raw_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < CHUNK_HEADER_SIZE {
            return None;
        }
        let start_tick = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let end_tick = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let event_count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let data_len = CHUNK_HEADER_SIZE + event_count * 12;
        if data.len() < data_len {
            return None;
        }

        let mut track_mask = [0u64; 4];
        for (i, mask) in track_mask.iter_mut().enumerate() {
            let off = 12 + i * 8;
            *mask = u64::from_le_bytes([
                data[off],
                data[off + 1],
                data[off + 2],
                data[off + 3],
                data[off + 4],
                data[off + 5],
                data[off + 6],
                data[off + 7],
            ]);
        }

        let event_bytes = &data[CHUNK_HEADER_SIZE..data_len];
        let events: Vec<CompactEvent> = event_bytes
            .chunks_exact(12)
            .map(|chunk| {
                let arr: &[u8; 12] = unsafe { &*(chunk.as_ptr() as *const [u8; 12]) };
                CompactEvent::from_bytes(arr)
            })
            .collect();

        Some(Self {
            start_tick,
            end_tick,
            events,
            track_mask,
        })
    }

    /// 从原始字节读取指定 tick 范围的事件（二进制搜索，不反序列化整块）
    ///
    /// 返回 (events_in_range, total_events_in_chunk)
    pub fn read_events_in_range(
        data: &[u8],
        from_tick: u32,
        to_tick: u32,
        max_events: usize,
    ) -> (Vec<CompactEvent>, u32) {
        if data.len() < CHUNK_HEADER_SIZE {
            return (Vec::new(), 0);
        }
        let event_count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        if event_count == 0 || data.len() < CHUNK_HEADER_SIZE + event_count * 12 {
            return (Vec::new(), event_count as u32);
        }

        let event_data = &data[CHUNK_HEADER_SIZE..];
        let event_limit = if max_events == 0 {
            usize::MAX
        } else {
            max_events
        };

        // 二分查找起始 tick
        let start_idx = binary_search_first_tick(event_data, from_tick);
        if start_idx >= event_count {
            return (Vec::new(), event_count as u32);
        }

        // 从 start_idx 开始读取，直到 to_tick 或 max_events
        let max_end = event_count.min(start_idx + event_limit);
        let mut result = Vec::with_capacity(max_end - start_idx);

        for i in start_idx..max_end {
            let off = i * 12;
            let ev = CompactEvent::from_bytes(unsafe {
                &*(event_data[off..off + 12].as_ptr() as *const [u8; 12])
            });
            if ev.delta_tick() >= to_tick {
                break;
            }
            result.push(ev);
        }

        (result, event_count as u32)
    }
}

/// 在原始事件字节数组中二分查找第一个 tick >= target 的事件索引
fn binary_search_first_tick(event_data: &[u8], target_tick: u32) -> usize {
    let count = event_data.len() / 12;
    let mut low = 0usize;
    let mut high = count;
    while low < high {
        let mid = low + (high - low) / 2;
        let off = mid * 12;
        let tick = u32::from_le_bytes([
            event_data[off],
            event_data[off + 1],
            event_data[off + 2],
            event_data[off + 3],
        ]);
        if tick < target_tick {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

/// 将 MIDI 文件原始数据流式分块写入输出文件（通过路径）。
///
/// 区别于 `chunk_midi_data_streaming`，此函数直接操作文件，
/// 适合搭配 `FileBackend` 使用，避免中间 copy。
pub fn chunk_midi_data_streaming_to_path(
    file_data: &[u8],
    output_path: &Path,
    progress: Option<&dyn Fn(f64)>,
) -> Result<(Vec<ChunkIndexRawEntry>, u32, u16), String> {
    let (tmp_dir, bucket_counters, track_count, total_ticks) =
        phase1_bucketize(file_data, progress)?;
    let file = std::fs::File::create(output_path)
        .map_err(|e| format!("创建输出文件 {output_path:?} 失败: {e}"))?;
    let mut writer = std::io::BufWriter::new(file);
    let entries = phase2_assemble(&tmp_dir, &bucket_counters, &mut writer)?;
    writer
        .into_inner()
        .map_err(|e| format!("flush 输出文件失败: {e}"))?;
    Ok((entries, total_ticks, track_count))
}

/// 第 1 遍：解析 MIDI 并将事件分发到桶文件。
///
/// 返回 (bucket_dir, bucket_counters, track_count, total_ticks)。
/// 注意：此函数返回后，`file_data` 可安全释放。
/// Phase 2 完全不依赖 `file_data`。
pub fn phase1_bucketize(
    file_data: &[u8],
    progress: Option<&dyn Fn(f64)>,
) -> Result<(PathBuf, Vec<u64>, u16, u32), String> {
    use std::fs::File;
    use std::io::BufWriter;

    let (header, track_iters) =
        midly::parse(file_data).map_err(|e| format!("MIDI 解析失败: {e}"))?;

    let event_iters: Vec<midly::EventIter> = track_iters
        .collect::<midly::Result<Vec<midly::EventIter>>>()
        .map_err(|e| format!("解析音轨失败: {e}"))?;
    let track_count = event_iters.len() as u16;

    let _division = match header.timing {
        Timing::Metrical(t) => t.as_int() as u32,
        _ => 480,
    };

    let tmp_dir = create_tmp_dir()?;
    let mut bucket_counters: Vec<u64> = vec![0u64; NUM_BUCKETS as usize];

    let mut buckets: Vec<BufWriter<File>> = (0..NUM_BUCKETS)
        .map(|b| {
            let path = bucket_path(&tmp_dir, b);
            let file = File::create(&path).map_err(|e| format!("创建桶文件 {path:?} 失败: {e}"))?;
            Ok(BufWriter::new(file))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut total_ticks = 0u32;
    for (track_idx, event_iter) in event_iters.into_iter().enumerate() {
        let mut current_tick = 0u32;
        for event_result in event_iter {
            let track_event =
                event_result.map_err(|e| format!("音轨 {track_idx} 事件解析失败: {e}"))?;
            current_tick = current_tick.saturating_add(u32::from(track_event.delta));

            if current_tick > total_ticks {
                total_ticks = current_tick;
            }

            if let Some(compact) =
                midi_event_to_compact(track_idx as u16, current_tick, &track_event.kind)
            {
                let chunk_idx = current_tick / params::CHUNK_TICK_SPAN;
                let bucket = chunk_idx % NUM_BUCKETS;
                let writer = &mut buckets[bucket as usize];
                writer
                    .write_all(&chunk_idx.to_le_bytes())
                    .map_err(|e| format!("写入桶文件失败: {e}"))?;
                writer
                    .write_all(compact.as_bytes())
                    .map_err(|e| format!("写入桶文件失败: {e}"))?;
                bucket_counters[bucket as usize] += 1;
            }
        }
        if let Some(cb) = progress {
            cb((track_idx + 1) as f64 / track_count as f64);
        }
    }

    drop(buckets);
    Ok((tmp_dir, bucket_counters, track_count, total_ticks))
}

/// 第 2 遍：从桶文件读取、并行构建 Chunk、直接写入文件路径。
///
/// 零内存累积 + 零中间 copy：每个 chunk 处理完立即写入输出文件。
pub fn phase2_assemble_to_path(
    tmp_dir: &Path,
    bucket_counters: &[u64],
    output_path: &Path,
) -> Result<Vec<ChunkIndexRawEntry>, String> {
    use std::fs;
    use std::io::Seek;
    use std::sync::Mutex;

    let output_file = std::fs::File::create(output_path)
        .map_err(|e| format!("创建输出文件 {output_path:?} 失败: {e}"))?;
    let shared_file = Arc::new(Mutex::new(output_file));
    let next_bucket = Mutex::new(0u32);
    let entries = Mutex::new(Vec::<ChunkIndexRawEntry>::new());

    std::thread::scope(|s| {
        for _ in 0..2u32 {
            s.spawn(|| {
                loop {
                    let bucket = {
                        let mut n = next_bucket.lock().unwrap();
                        if *n >= NUM_BUCKETS {
                            return;
                        }
                        let b = *n;
                        *n += 1;
                        b
                    };
                    if bucket_counters[bucket as usize] == 0 {
                        continue;
                    }

                    let path = bucket_path(tmp_dir, bucket);
                    let Ok(bucket_data) = fs::read(&path) else {
                        continue;
                    };

                    let mut chunk_map: std::collections::HashMap<u32, Vec<CompactEvent>> =
                        std::collections::HashMap::with_capacity(4);
                    let mut off = 0usize;
                    while off + 16 <= bucket_data.len() {
                        let idx = u32::from_le_bytes([
                            bucket_data[off],
                            bucket_data[off + 1],
                            bucket_data[off + 2],
                            bucket_data[off + 3],
                        ]);
                        let mut eb = [0u8; 12];
                        eb.copy_from_slice(&bucket_data[off + 4..off + 16]);
                        chunk_map
                            .entry(idx)
                            .or_default()
                            .push(CompactEvent::from_bytes(&eb));
                        off += 16;
                    }
                    drop(bucket_data);

                    let mut keys: Vec<u32> = chunk_map.keys().copied().collect();
                    keys.sort_unstable();

                    for &ck in &keys {
                        let events = chunk_map.remove(&ck).unwrap_or_default();
                        let chunk = EventChunk::new(ck * params::CHUNK_TICK_SPAN, events);
                        let bytes = chunk.to_raw_bytes();

                        let sf = shared_file.clone();
                        let mut f = sf.lock().unwrap();
                        let offset = f.seek(SeekFrom::End(0)).expect("seek") as u64;
                        f.write_all(&bytes).expect("write");
                        drop(f);

                        entries.lock().unwrap().push(ChunkIndexRawEntry {
                            start_tick: chunk.start_tick,
                            file_offset: offset,
                            byte_length: bytes.len() as u32,
                            track_mask_low: chunk.track_mask[0],
                            track_mask_high: chunk.track_mask[1],
                        });
                    }
                    let _ = fs::remove_file(&path);
                }
            });
        }
    });

    drop(shared_file);
    let mut entries = entries.into_inner().unwrap();
    entries.sort_by_key(|e| e.start_tick);
    let _ = fs::remove_dir_all(tmp_dir);
    Ok(entries)
}

/// 第 2 遍：从桶文件读取、并行构建 Chunk、串行写入输出。
///
/// 零内存累积：每个 chunk 处理完立即写入共享文件，
/// 不保留任何中间数据在内存。
///
/// # 参数
/// - `tmp_dir`: phase1_bucketize 返回的临时目录
/// - `bucket_counters`: 每个桶的事件计数
/// - `output`: 输出流
pub fn phase2_assemble<W: Write>(
    tmp_dir: &Path,
    bucket_counters: &[u64],
    output: &mut W,
) -> Result<Vec<ChunkIndexRawEntry>, String> {
    use std::fs;
    use std::io::Seek;
    use std::sync::Mutex;

    let tmp_chunk_data = {
        let mut p = std::env::temp_dir();
        p.push(format!("lumino_chunk_data_{:016x}", rand_fallback()));
        p
    };
    let shared_file = Arc::new(Mutex::new(
        std::fs::File::create(&tmp_chunk_data).map_err(|e| format!("创建共享输出文件失败: {e}"))?,
    ));

    let next_bucket = Mutex::new(0u32);
    let entries = Mutex::new(Vec::<ChunkIndexRawEntry>::new());

    std::thread::scope(|s| {
        for _ in 0..2u32 {
            s.spawn(|| {
                loop {
                    let bucket = {
                        let mut n = next_bucket.lock().unwrap();
                        if *n >= NUM_BUCKETS {
                            return;
                        }
                        let b = *n;
                        *n += 1;
                        b
                    };
                    if bucket_counters[bucket as usize] == 0 {
                        continue;
                    }

                    let path = bucket_path(tmp_dir, bucket);
                    let Ok(bucket_data) = fs::read(&path) else {
                        continue;
                    };

                    let mut chunk_map: std::collections::HashMap<u32, Vec<CompactEvent>> =
                        std::collections::HashMap::with_capacity(4);
                    let mut off = 0usize;
                    while off + 16 <= bucket_data.len() {
                        let idx = u32::from_le_bytes([
                            bucket_data[off],
                            bucket_data[off + 1],
                            bucket_data[off + 2],
                            bucket_data[off + 3],
                        ]);
                        let mut eb = [0u8; 12];
                        eb.copy_from_slice(&bucket_data[off + 4..off + 16]);
                        chunk_map
                            .entry(idx)
                            .or_default()
                            .push(CompactEvent::from_bytes(&eb));
                        off += 16;
                    }
                    drop(bucket_data);

                    let mut keys: Vec<u32> = chunk_map.keys().copied().collect();
                    keys.sort_unstable();

                    for &ck in &keys {
                        let events = chunk_map.remove(&ck).unwrap_or_default();
                        let chunk = EventChunk::new(ck * params::CHUNK_TICK_SPAN, events);
                        let bytes = chunk.to_raw_bytes();

                        let sf = shared_file.clone();
                        let mut f = sf.lock().unwrap();
                        let offset = f.seek(SeekFrom::End(0)).expect("seek") as u64;
                        f.write_all(&bytes).expect("write");
                        drop(f);

                        entries.lock().unwrap().push(ChunkIndexRawEntry {
                            start_tick: chunk.start_tick,
                            file_offset: offset,
                            byte_length: bytes.len() as u32,
                            track_mask_low: chunk.track_mask[0],
                            track_mask_high: chunk.track_mask[1],
                        });
                    }

                    let _ = fs::remove_file(&path);
                }
            });
        }
    });

    drop(shared_file);
    let mut chunk_file =
        std::fs::File::open(&tmp_chunk_data).map_err(|e| format!("读取共享输出文件失败: {e}"))?;
    std::io::copy(&mut chunk_file, output).map_err(|e| format!("拷贝输出失败: {e}"))?;
    let _ = std::fs::remove_file(&tmp_chunk_data);

    let mut entries = entries.into_inner().unwrap();
    entries.sort_by_key(|e| e.start_tick);

    // 清理桶目录
    let _ = fs::remove_dir_all(tmp_dir);

    Ok(entries)
}

/// 合并 Phase 1 + Phase 2（兼容旧 API，不分割释放时机）
pub fn chunk_midi_data_streaming<W: Write>(
    file_data: &[u8],
    output: &mut W,
    progress: Option<&dyn Fn(f64)>,
) -> Result<(Vec<ChunkIndexRawEntry>, u32, u16), String> {
    let (tmp_dir, bucket_counters, track_count, total_ticks) =
        phase1_bucketize(file_data, progress)?;
    let entries = phase2_assemble(&tmp_dir, &bucket_counters, output)?;
    Ok((entries, total_ticks, track_count))
}

/// 流式分块并返回 Vec<EventChunk>（小文件兼容包装器）
///
/// 内部使用 `chunk_midi_data_streaming`。会在内存中保留所有 chunks，
/// 因此仅适用于小文件或测试场景。黑乐谱应用请直接使用流式 API。
pub fn chunk_midi_data(
    file_data: &[u8],
    progress: Option<&dyn Fn(f64)>,
) -> Result<(Vec<EventChunk>, u32, u16), String> {
    let mut buffer = Vec::new();
    let (raw_entries, total_ticks, track_count) =
        chunk_midi_data_streaming(file_data, &mut buffer, progress)?;

    // 从 buffer 中反序列化 chunks
    let mut chunks = Vec::with_capacity(raw_entries.len());
    let mut offset = 0usize;
    for entry in &raw_entries {
        let end = offset + entry.byte_length as usize;
        let chunk = EventChunk::from_raw_bytes(&buffer[offset..end])
            .ok_or_else(|| format!("反序列化 chunk 失败: 偏移 {offset}"))?;
        chunks.push(chunk);
        offset = end;
    }

    Ok((chunks, total_ticks, track_count))
}

/// 原始块索引条目（序列化前的轻量表示）
#[derive(Debug, Clone, Copy)]
pub struct ChunkIndexRawEntry {
    pub start_tick: u32,
    pub file_offset: u64,
    pub byte_length: u32,
    pub track_mask_low: u64,
    pub track_mask_high: u64,
}

// ── 辅助函数 ──

fn create_tmp_dir() -> Result<PathBuf, String> {
    let mut path = std::env::temp_dir();
    path.push(format!("lumino_cache_{:016x}", rand_fallback()));
    std::fs::create_dir_all(&path).map_err(|e| format!("创建临时目录失败: {e}"))?;
    Ok(path)
}

fn bucket_path(tmp_dir: &Path, bucket: u32) -> PathBuf {
    tmp_dir.join(format!("b{:03x}.tmp", bucket))
}

fn rand_fallback() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_nanos() as u64) ^ (now.as_nanos() >> 32) as u64
}

/// 将 midly 的 TrackEventKind 转换为 CompactEvent
fn midi_event_to_compact(track_id: u16, tick: u32, kind: &TrackEventKind) -> Option<CompactEvent> {
    match kind {
        TrackEventKind::Midi { channel, message } => {
            let ch = channel.as_int();
            match message {
                MidiMessage::NoteOn { key, vel } => Some(CompactEvent::new(
                    tick,
                    track_id,
                    EventKind::NoteOn,
                    ch,
                    key.as_int() as u16,
                    vel.as_int() as u16,
                )),
                MidiMessage::NoteOff { key, vel } => Some(CompactEvent::new(
                    tick,
                    track_id,
                    EventKind::NoteOff,
                    ch,
                    key.as_int() as u16,
                    vel.as_int() as u16,
                )),
                MidiMessage::Controller { controller, value } => Some(CompactEvent::new(
                    tick,
                    track_id,
                    EventKind::ControlChange,
                    ch,
                    controller.as_int() as u16,
                    value.as_int() as u16,
                )),
                MidiMessage::ProgramChange { program } => Some(CompactEvent::new(
                    tick,
                    track_id,
                    EventKind::ProgramChange,
                    ch,
                    program.as_int() as u16,
                    0,
                )),
                MidiMessage::PitchBend { bend } => Some(CompactEvent::new(
                    tick,
                    track_id,
                    EventKind::PitchBend,
                    ch,
                    bend.as_int() as u16,
                    0,
                )),
                MidiMessage::Aftertouch { key, vel } => Some(CompactEvent::new(
                    tick,
                    track_id,
                    EventKind::Aftertouch,
                    ch,
                    key.as_int() as u16,
                    vel.as_int() as u16,
                )),
                _ => None,
            }
        }
        TrackEventKind::Meta(meta) => match meta {
            MetaMessage::Tempo(tempo) => {
                let t = tempo.as_int();
                Some(CompactEvent::new(
                    tick,
                    track_id,
                    EventKind::Tempo,
                    0,
                    (t & 0xFFFF) as u16,
                    ((t >> 16) & 0xFFFF) as u16,
                ))
            }
            MetaMessage::TimeSignature(num, den, _, _) => Some(CompactEvent::new(
                tick,
                track_id,
                EventKind::TimeSignature,
                0,
                *num as u16,
                *den as u16,
            )),
            MetaMessage::KeySignature(key, is_major) => Some(CompactEvent::new(
                tick,
                track_id,
                EventKind::KeySignature,
                0,
                *key as u16,
                *is_major as u16,
            )),
            _ => None,
        },
        TrackEventKind::SysEx(data) => Some(CompactEvent::new(
            tick,
            track_id,
            EventKind::SysEx,
            0,
            data.len() as u16,
            0,
        )),
        TrackEventKind::Escape(data) => Some(CompactEvent::new(
            tick,
            track_id,
            EventKind::Other,
            0,
            data.len() as u16,
            0,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_events(count: usize, start_tick: u32) -> Vec<CompactEvent> {
        (0..count)
            .map(|i| {
                CompactEvent::new(
                    start_tick + i as u32 * 100,
                    (i % 4) as u16,
                    EventKind::NoteOn,
                    (i % 16) as u8,
                    60 + (i % 24) as u16,
                    100,
                )
            })
            .collect()
    }

    #[test]
    fn test_event_chunk_new() {
        let events = sample_events(10, 0);
        let chunk = EventChunk::new(0, events);
        assert_eq!(chunk.start_tick, 0);
        assert_eq!(chunk.end_tick, params::CHUNK_TICK_SPAN);
        assert!(!chunk.is_empty());
        assert_eq!(chunk.len(), 10);
    }

    #[test]
    fn test_event_chunk_empty() {
        let chunk = EventChunk::new(0, vec![]);
        assert!(chunk.is_empty());
    }

    #[test]
    fn test_event_chunk_track_mask() {
        let events = vec![
            CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
            CompactEvent::new(100, 5, EventKind::NoteOn, 0, 64, 100),
            CompactEvent::new(200, 200, EventKind::NoteOn, 0, 67, 100),
        ];
        let chunk = EventChunk::new(0, events);
        assert!(chunk.has_track(0));
        assert!(chunk.has_track(5));
        assert!(chunk.has_track(200));
        assert!(!chunk.has_track(256));
    }

    #[test]
    fn test_event_chunk_serialize_roundtrip() {
        let events = sample_events(100, 0);
        let chunk = EventChunk::new(0, events);
        let bytes = chunk.to_raw_bytes();
        let restored = EventChunk::from_raw_bytes(&bytes).unwrap();
        assert_eq!(restored.start_tick, chunk.start_tick);
        assert_eq!(restored.len(), chunk.len());
    }

    #[test]
    fn test_chunk_midi_data_basic() {
        let midi_bytes = create_minimal_midi();
        let (chunks, total_ticks, track_count) = chunk_midi_data(&midi_bytes, None).unwrap();
        assert_eq!(track_count, 1);
        assert!(total_ticks > 0);
        let total_events: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total_events, 2);
    }

    #[test]
    fn test_streaming_roundtrip() {
        let midi_bytes = create_minimal_midi();
        let mut output = Vec::new();
        let (raw_entries, total_ticks, track_count) =
            chunk_midi_data_streaming(&midi_bytes, &mut output, None).unwrap();
        assert_eq!(track_count, 1);
        assert!(total_ticks > 0);
        assert!(!raw_entries.is_empty());
        // Verify deserialization
        for entry in &raw_entries {
            let start = entry.file_offset as usize;
            let end = start + entry.byte_length as usize;
            let chunk = EventChunk::from_raw_bytes(&output[start..end]).unwrap();
            assert_eq!(chunk.start_tick, entry.start_tick);
        }
    }

    fn create_minimal_midi() -> Vec<u8> {
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track_data = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x90, 0x3C, 0x64, 0x80, 0x3C,
            0x00, 0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut result = Vec::with_capacity(header.len() + track_data.len());
        result.extend_from_slice(&header);
        result.extend_from_slice(&track_data);
        result
    }

    #[test]
    fn test_midi_event_to_compact_note_on() {
        let kind = TrackEventKind::Midi {
            channel: midly::num::u4::new(5),
            message: MidiMessage::NoteOn {
                key: midly::num::u7::new(60),
                vel: midly::num::u7::new(100),
            },
        };
        let compact = midi_event_to_compact(0, 1000, &kind).unwrap();
        assert_eq!(compact.kind(), EventKind::NoteOn);
        assert_eq!(compact.channel(), 5);
        assert_eq!(compact.param1(), 60);
        assert_eq!(compact.param2(), 100);
    }
}
