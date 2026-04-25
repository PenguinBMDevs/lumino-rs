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

use std::io::Write;
use std::mem;
use std::path::{Path, PathBuf};

use lumino_midi::compact::{CompactEvent, EventKind};
use midly::{MetaMessage, MidiMessage, Timing, TrackEventKind};
use serde::{Deserialize, Serialize};

use crate::params;

/// 外部排序的桶数量
const NUM_BUCKETS: u32 = 64;

/// 事件块 — 固定 tick 跨度的事件集合
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

/// 将 MIDI 文件原始数据流式分块并串行化写入输出流。
///
/// 使用 64 桶外部排序，一次只保留一个桶的数据在内存中。
/// 第 1 遍：解析 MIDI 并将事件分发到桶文件
/// 第 2 遍：按桶读取、排序、构建 Chunk、序列化到输出
///
/// # 内存保证
/// - 第 1 遍：midi 文件数据 (~1.25GB, 临时) + 64 个桶文件句柄 + 少量计数
/// - 第 2 遍：midi 数据已释放，仅一个桶的数据在内存 (约总事件/64)
///
/// # 参数
/// - `file_data`: 完整的 .mid 文件字节
/// - `output`: 输出流（序列化的 EventChunk 依次写入）
/// - `progress`: 可选进度回调（0.0 ~ 1.0）
///
/// # 返回值
/// (ChunkIndex 原始条目, total_ticks, track_count)
pub fn chunk_midi_data_streaming<W: Write>(
    file_data: &[u8],
    output: &mut W,
    progress: Option<&dyn Fn(f64)>,
) -> Result<(Vec<ChunkIndexRawEntry>, u32, u16), String> {
    use std::fs::{self, File};
    use std::io::BufWriter;

    // ── 第 1 遍：解析 MIDI，分发事件到桶文件 ──
    let (header, track_iters) =
        midly::parse(file_data).map_err(|e| format!("MIDI 解析失败: {e}"))?;

    // 收集 EventIters 以支持进度回调（需要知道 total_tracks）
    let event_iters: Vec<midly::EventIter> = track_iters
        .collect::<midly::Result<Vec<midly::EventIter>>>()
        .map_err(|e| format!("解析音轨失败: {e}"))?;
    let track_count = event_iters.len() as u16;

    let _division = match header.timing {
        Timing::Metrical(t) => t.as_int() as u32,
        _ => 480,
    };

    // 创建临时目录
    let tmp_dir = create_tmp_dir()?;
    let mut bucket_counters: Vec<u64> = vec![0u64; NUM_BUCKETS as usize];

    // 打开桶文件
    let mut buckets: Vec<BufWriter<File>> = (0..NUM_BUCKETS)
        .map(|b| {
            let path = bucket_path(&tmp_dir, b);
            let file = File::create(&path).map_err(|e| format!("创建桶文件 {path:?} 失败: {e}"))?;
            Ok(BufWriter::new(file))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // 逐音轨解析并分发
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

                // 写入 chunk_idx (4 bytes) + CompactEvent (12 bytes)
                let chunk_bytes = chunk_idx.to_le_bytes();
                writer
                    .write_all(&chunk_bytes)
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

    // 关闭所有桶文件
    drop(buckets);

    // ── 第 2 遍：按桶读取、O(N) 分组、构建 Chunk ──
    let mut entries: Vec<ChunkIndexRawEntry> = Vec::new();
    let mut current_file_offset: u64 = 0;

    for bucket in 0..NUM_BUCKETS {
        let count = bucket_counters[bucket as usize];
        if count == 0 {
            continue;
        }

        // 读取整个桶文件
        let path = bucket_path(&tmp_dir, bucket);
        let bucket_data = fs::read(&path).map_err(|e| format!("读取桶文件 {path:?} 失败: {e}"))?;

        // O(N) 分组：直接按 chunk_idx 分发到 HashMap（每桶仅 1-3 个 chunk）
        // 避免 O(N log N) 排序
        let mut chunk_map: std::collections::HashMap<u32, Vec<CompactEvent>> =
            std::collections::HashMap::with_capacity(4);
        let mut offset = 0usize;
        while offset + 16 <= bucket_data.len() {
            let idx = u32::from_le_bytes([
                bucket_data[offset],
                bucket_data[offset + 1],
                bucket_data[offset + 2],
                bucket_data[offset + 3],
            ]);
            let mut evt_bytes = [0u8; 12];
            evt_bytes.copy_from_slice(&bucket_data[offset + 4..offset + 16]);
            let event = CompactEvent::from_bytes(&evt_bytes);

            chunk_map.entry(idx).or_default().push(event);
            offset += 16;
        }

        // 按 chunk_idx 排序 keys，确保索引顺序一致
        let mut chunk_indices: Vec<u32> = chunk_map.keys().copied().collect();
        chunk_indices.sort_unstable();

        for chunk_idx in chunk_indices {
            let events = chunk_map.remove(&chunk_idx).unwrap_or_default();
            let start_tick = chunk_idx * params::CHUNK_TICK_SPAN;

            // 构建 EventChunk（同时计算 track_mask）
            let chunk = EventChunk::new(start_tick, events);
            let serialized = chunk
                .to_bytes()
                .map_err(|e| format!("序列化 chunk {chunk_idx} 失败: {e}"))?;

            entries.push(ChunkIndexRawEntry {
                start_tick,
                file_offset: current_file_offset,
                byte_length: serialized.len() as u32,
                track_mask_low: chunk.track_mask[0],
                track_mask_high: chunk.track_mask[1],
            });

            output
                .write_all(&serialized)
                .map_err(|e| format!("写入输出流失败: {e}"))?;
            current_file_offset += serialized.len() as u64;
        }

        // 删除桶文件
        let _ = fs::remove_file(&path);
    }

    // 清理临时目录
    let _ = fs::remove_dir_all(&tmp_dir);

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
        let chunk = EventChunk::from_bytes(&buffer[offset..end])
            .map_err(|e| format!("反序列化 chunk 失败: {e}"))?;
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
        let bytes = chunk.to_bytes().unwrap();
        let restored = EventChunk::from_bytes(&bytes).unwrap();
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
            let chunk = EventChunk::from_bytes(&output[start..end]).unwrap();
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
