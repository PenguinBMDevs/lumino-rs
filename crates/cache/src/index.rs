//! 两级索引 — ChunkIndex
//!
//! 常驻内存的块索引，支持：
//! - 从 .mid 首次扫描构建
//! - 从 .black 格式直接读取预计算索引
//!
//! 内存占用预估：
//! - 每个 ChunkIndexEntry: 32 字节
//! - 5000 块 ≈ 160 KB（常驻内存，忽略不计）
//!
//! .black 文件格式：
//! ```text
//! offset  size  field
//!  0       8    MAGIC = "LUMIBLK1"
//!  8       4    format_version (u32 LE)
//! 12       4    total_ticks (u32 LE)
//! 16       2    track_count (u16 LE)
//! 18       2    _reserved
//! 20       4    entry_count (u32 LE)
//! 24       N    entries[entry_count] (ChunkIndexEntry × entry_count)
//! ```

use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::chunk::EventChunk;
use crate::params;

/// 块索引条目（32 字节）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct ChunkIndexEntry {
    /// 块起始 tick
    pub start_tick: u32,
    /// 文件偏移（在 .black 文件中或 L3 后端中的位置）
    pub file_offset: u64,
    /// 序列化后字节长度
    pub byte_length: u32,
    /// 音轨掩码（位 0-63 表示音轨 0-63 在该块中有事件）
    pub track_mask_low: u64,
    /// 音轨掩码（位 64-127）
    pub track_mask_high: u64,
}

impl ChunkIndexEntry {
    /// 创建新的索引条目
    pub fn new(start_tick: u32, file_offset: u64, byte_length: u32, track_mask: &[u64; 4]) -> Self {
        Self {
            start_tick,
            file_offset,
            byte_length,
            track_mask_low: track_mask[0],
            track_mask_high: track_mask[1],
        }
    }

    /// 检查是否有任何音轨在当前块中
    pub fn has_any_track(&self) -> bool {
        self.track_mask_low != 0 || self.track_mask_high != 0
    }

    /// 检查指定音轨是否在此块中
    pub fn has_track(&self, track_id: u16) -> bool {
        let tid = track_id as usize;
        if tid < 64 {
            (self.track_mask_low >> tid) & 1 != 0
        } else if tid < 128 {
            (self.track_mask_high >> (tid - 64)) & 1 != 0
        } else {
            false
        }
    }
}

/// 常驻内存的块索引
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkIndex {
    /// 索引条目
    pub entries: Vec<ChunkIndexEntry>,
    /// 文件总 tick 数
    pub total_ticks: u32,
    /// 音轨数量
    pub track_count: u16,
    /// 每个块覆盖的 tick 数
    pub tick_span: u32,
}

impl ChunkIndex {
    /// 创建空的索引
    pub fn new(total_ticks: u32, track_count: u16) -> Self {
        Self {
            entries: Vec::new(),
            total_ticks,
            track_count,
            tick_span: params::CHUNK_TICK_SPAN,
        }
    }

    /// 通过扫描完整块列表构建索引
    pub fn from_chunks(chunks: &[EventChunk], total_ticks: u32, track_count: u16) -> Self {
        let mut entries = Vec::with_capacity(chunks.len());
        let mut file_offset: u64 = 0;

        for chunk in chunks {
            let serialized = chunk.to_bytes().unwrap_or_default();
            let byte_length = serialized.len() as u32;

            entries.push(ChunkIndexEntry::new(
                chunk.start_tick,
                file_offset,
                byte_length,
                &chunk.track_mask,
            ));

            file_offset += byte_length as u64;
        }

        Self {
            entries,
            total_ticks,
            track_count,
            tick_span: params::CHUNK_TICK_SPAN,
        }
    }

    /// 从流式分块输出的原始条目构建索引
    pub fn from_raw_entries(
        entries: Vec<crate::chunk::ChunkIndexRawEntry>,
        total_ticks: u32,
        track_count: u16,
    ) -> Self {
        let index_entries = entries
            .iter()
            .map(|e| ChunkIndexEntry {
                start_tick: e.start_tick,
                file_offset: e.file_offset,
                byte_length: e.byte_length,
                track_mask_low: e.track_mask_low,
                track_mask_high: e.track_mask_high,
            })
            .collect();
        Self {
            entries: index_entries,
            total_ticks,
            track_count,
            tick_span: params::CHUNK_TICK_SPAN,
        }
    }

    /// 总块数
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 索引是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 通过 tick 查找对应的块索引
    ///
    /// 二分查找，O(log N)。
    /// 返回该 tick 所属块的 entry 索引。
    pub fn find_chunk_index(&self, tick: u32) -> Option<usize> {
        if self.entries.is_empty() {
            return None;
        }
        let chunk_idx = tick / self.tick_span;
        // entries 是按 start_tick 排序的连续块
        if (chunk_idx as usize) < self.entries.len() {
            Some(chunk_idx as usize)
        } else {
            None
        }
    }

    /// 返回指定 tick 范围的块索引范围 [start, end)
    pub fn chunk_range(&self, from_tick: u32, to_tick: u32) -> (usize, usize) {
        let from_idx = (from_tick / self.tick_span) as usize;
        let to_idx = to_tick.div_ceil(self.tick_span) as usize;
        let start = from_idx.min(self.entries.len());
        let end = to_idx.min(self.entries.len());
        (start, end)
    }

    /// 写入 .black 格式索引文件
    pub fn write_black(&self, writer: &mut impl Write) -> io::Result<()> {
        writer.write_all(params::INDEX_MAGIC)?;
        writer.write_all(&params::INDEX_FORMAT_VERSION.to_le_bytes())?;
        writer.write_all(&self.total_ticks.to_le_bytes())?;
        writer.write_all(&self.track_count.to_le_bytes())?;
        writer.write_all(&[0u8; 2])?; // reserved
        writer.write_all(&(self.entries.len() as u32).to_le_bytes())?;

        for entry in &self.entries {
            writer.write_all(&entry.start_tick.to_le_bytes())?;
            writer.write_all(&entry.file_offset.to_le_bytes())?;
            writer.write_all(&entry.byte_length.to_le_bytes())?;
            writer.write_all(&entry.track_mask_low.to_le_bytes())?;
            writer.write_all(&entry.track_mask_high.to_le_bytes())?;
        }

        Ok(())
    }

    /// 从 .black 格式文件读取索引
    pub fn read_black(reader: &mut impl io::Read) -> io::Result<Self> {
        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        if &magic != params::INDEX_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "无效的 .black 魔数: 期望 {:02x?}, 得到 {:02x?}",
                    params::INDEX_MAGIC,
                    magic
                ),
            ));
        }

        let mut version_buf = [0u8; 4];
        reader.read_exact(&mut version_buf)?;
        let _version = u32::from_le_bytes(version_buf);

        let mut buf4 = [0u8; 4];
        let mut buf2 = [0u8; 2];

        reader.read_exact(&mut buf4)?;
        let total_ticks = u32::from_le_bytes(buf4);

        reader.read_exact(&mut buf2)?;
        let track_count = u16::from_le_bytes(buf2);

        let mut _reserved = [0u8; 2];
        reader.read_exact(&mut _reserved)?;

        reader.read_exact(&mut buf4)?;
        let entry_count = u32::from_le_bytes(buf4) as usize;

        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            reader.read_exact(&mut buf4)?;
            let start_tick = u32::from_le_bytes(buf4);

            let mut buf8 = [0u8; 8];
            reader.read_exact(&mut buf8)?;
            let file_offset = u64::from_le_bytes(buf8);

            reader.read_exact(&mut buf4)?;
            let byte_length = u32::from_le_bytes(buf4);

            reader.read_exact(&mut buf8)?;
            let track_mask_low = u64::from_le_bytes(buf8);

            reader.read_exact(&mut buf8)?;
            let track_mask_high = u64::from_le_bytes(buf8);

            entries.push(ChunkIndexEntry {
                start_tick,
                file_offset,
                byte_length,
                track_mask_low,
                track_mask_high,
            });
        }

        Ok(Self {
            entries,
            total_ticks,
            track_count,
            tick_span: params::CHUNK_TICK_SPAN,
        })
    }

    /// 保存 .black 索引到文件
    pub fn save_black(&self, path: &Path) -> io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);
        self.write_black(&mut writer)
    }

    /// 从 .black 文件加载索引
    pub fn load_black(path: &Path) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        Self::read_black(&mut reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi::compact::{CompactEvent, EventKind};

    fn make_test_chunks() -> Vec<EventChunk> {
        let mut chunks = Vec::new();
        for i in 0..3 {
            let start = i * params::CHUNK_TICK_SPAN;
            let events = vec![
                CompactEvent::new(start, 0, EventKind::NoteOn, 0, 60, 100),
                CompactEvent::new(start + 100, 1, EventKind::NoteOff, 0, 60, 0),
            ];
            chunks.push(EventChunk::new(start, events));
        }
        chunks
    }

    #[test]
    fn test_index_from_chunks() {
        let chunks = make_test_chunks();
        let index = ChunkIndex::from_chunks(&chunks, 3 * params::CHUNK_TICK_SPAN, 2);
        assert_eq!(index.len(), 3);
        assert_eq!(index.track_count, 2);
    }

    #[test]
    fn test_find_chunk() {
        let chunks = make_test_chunks();
        let index = ChunkIndex::from_chunks(&chunks, 3 * params::CHUNK_TICK_SPAN, 2);

        let idx = index.find_chunk_index(0).unwrap();
        assert_eq!(idx, 0);

        let idx = index.find_chunk_index(params::CHUNK_TICK_SPAN).unwrap();
        assert_eq!(idx, 1);

        let idx = index
            .find_chunk_index(params::CHUNK_TICK_SPAN + 50)
            .unwrap();
        assert_eq!(idx, 1);
    }

    #[test]
    fn test_chunk_range() {
        let chunks = make_test_chunks();
        let index = ChunkIndex::from_chunks(&chunks, 3 * params::CHUNK_TICK_SPAN, 2);

        let (start, end) = index.chunk_range(0, params::CHUNK_TICK_SPAN);
        assert_eq!(start, 0);
        assert_eq!(end, 1);

        // Cross-chunk range
        let mid = params::CHUNK_TICK_SPAN / 2;
        let (start, end) = index.chunk_range(mid, mid + params::CHUNK_TICK_SPAN);
        assert_eq!(start, 0);
        assert_eq!(end, 2);
    }

    #[test]
    fn test_black_roundtrip() {
        let chunks = make_test_chunks();
        let index = ChunkIndex::from_chunks(&chunks, 3 * params::CHUNK_TICK_SPAN, 2);

        let mut buffer = Vec::new();
        index.write_black(&mut buffer).unwrap();

        let restored = ChunkIndex::read_black(&mut &buffer[..]).unwrap();
        assert_eq!(restored.len(), index.len());
        assert_eq!(restored.total_ticks, index.total_ticks);
        assert_eq!(restored.track_count, index.track_count);

        for (a, b) in index.entries.iter().zip(restored.entries.iter()) {
            assert_eq!(a.start_tick, b.start_tick);
            assert_eq!(a.byte_length, b.byte_length);
        }
    }

    #[test]
    fn test_entry_track_presence() {
        let entry = ChunkIndexEntry::new(0, 0, 100, &[1, 0, 0, 0]);
        assert!(entry.has_track(0));
        assert!(!entry.has_track(1));

        let entry2 = ChunkIndexEntry::new(0, 0, 100, &[0, 1, 0, 0]);
        assert!(entry2.has_track(64));
        assert!(!entry2.has_track(0));
    }

    #[test]
    fn test_index_out_of_range_tick() {
        let index = ChunkIndex::new(100_000, 1);
        assert!(index.find_chunk_index(999_999).is_none());
    }
}
