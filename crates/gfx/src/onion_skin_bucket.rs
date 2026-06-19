//! 洋葱皮音符列表 — 按时间分块，避免大 MIDI 下 GPU 全量上传
//!
//! 设计思路：
//! - 所有音符按 start_tick 排序后，划分为固定大小的 chunk
//! - 每个 chunk 记录 [tick_start, tick_end] 和 [note_start, note_end]
//! - 渲染时只上传与当前视口时间重叠的 chunk，CPU 扫描量从 N 降到 O(visible_chunks × chunk_size)
//! - 显存占用由总音符数决定，变为由视口大小决定

use lumino_midi_loader::{MidiDocument, NoteInfo};

use crate::OnionNote;

/// 单个时间块的元数据
#[derive(Debug, Clone, Copy, Default)]
pub struct OnionNoteChunk {
    /// 该块内最早音符的 start_tick
    pub tick_start: u32,
    /// 该块内最晚音符的 end_tick
    pub tick_end: u32,
    /// 在 notes 数组中的起始索引（包含）
    pub note_start: usize,
    /// 在 notes 数组中的结束索引（不包含）
    pub note_end: usize,
}

/// 洋葱皮音符列表 — 按时间分块的有序数组
///
/// 设计思路：
/// - 音符按 start_tick 排序，支持二分查找可见 chunk
/// - 每个 chunk 大小固定（CHUNK_SIZE），降低每帧 CPU 扫描和 GPU 上传量
/// - 滚动时只重新上传进入视口的新 chunk，静止帧零上传
#[derive(Debug, Clone)]
pub struct OnionNoteList {
    /// 排序后的扁平音符数组
    notes: Vec<OnionNote>,
    /// 时间分块索引
    chunks: Vec<OnionNoteChunk>,
    /// 数据版本号，每次 rebuild/update 后递增
    version: u64,
}

/// 每 chunk 音符数量。
/// 1 chunk × 16 字节/音符 = 16 MB；视口通常只覆盖 1-3 个 chunk，
/// 因此 GPU 常驻数据量从“总音符数”降为“≈ 3 × 16 MB”。
const CHUNK_SIZE: usize = 1_000_000;

impl Default for OnionNoteList {
    fn default() -> Self {
        Self::new()
    }
}

impl OnionNoteList {
    /// 创建空的音符列表
    #[must_use]
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            chunks: Vec::new(),
            version: 0,
        }
    }

    /// 当前数据版本号
    #[inline]
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// 音符数量
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// 是否为空
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// 获取完整音符切片（主要用于测试/调试）
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[OnionNote] {
        &self.notes
    }

    /// 获取 chunk 索引
    #[inline]
    #[must_use]
    pub fn chunks(&self) -> &[OnionNoteChunk] {
        &self.chunks
    }

    /// 获取所有音符（主要用于测试/调试）
    #[inline]
    #[must_use]
    pub fn notes(&self) -> &[OnionNote] {
        &self.notes
    }

    /// 清空所有音符
    pub fn clear(&mut self) {
        self.notes.clear();
        self.chunks.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// 从 MIDI 文档构建音符列表
    ///
    /// # 参数
    /// - `doc`: MIDI 文档
    /// - `track_filter`: 返回 true 的音轨才会被加入
    /// - `current_track`: 当前编辑音轨，自动排除
    #[must_use]
    pub fn from_midi_document(
        doc: &MidiDocument,
        track_filter: impl Fn(usize) -> bool,
        current_track: usize,
    ) -> Self {
        let mut list = Self::new();
        list.rebuild_from_midi_document(doc, track_filter, current_track);
        list
    }

    /// 从 MIDI 文档重建（会清空已有数据）
    pub fn rebuild_from_midi_document(
        &mut self,
        doc: &MidiDocument,
        track_filter: impl Fn(usize) -> bool,
        current_track: usize,
    ) {
        self.clear();

        for track_idx in 0..doc.track_count() {
            if track_idx == current_track {
                continue;
            }
            if !track_filter(track_idx) {
                continue;
            }
            self.add_midi_track_notes(doc, track_idx);
        }

        self.sort_and_rechunk();
        self.version = self.version.wrapping_add(1);
    }

    /// 添加/更新用户编辑音轨的音符
    ///
    /// 先移除该音轨已有的音符，再追加新音符，然后重新排序分块。
    pub fn update_user_track<'a>(
        &mut self,
        track_idx: u16,
        notes: impl Iterator<Item = &'a lumino_core::Note>,
    ) {
        self.remove_track(track_idx);

        for note in notes {
            if note.key > 255 {
                continue;
            }
            let start = note.tick as u32;
            let length = note.length.max(0.0) as u32;
            let end = start.saturating_add(length);
            self.notes
                .push(OnionNote::new(start, end, note.key as u8, track_idx));
        }

        self.sort_and_rechunk();
        self.version = self.version.wrapping_add(1);
    }

    /// 从列表中移除指定音轨的所有音符
    pub fn remove_track(&mut self, track_idx: u16) {
        let before = self.notes.len();
        self.notes.retain(|n| n.track_idx() != track_idx);
        if self.notes.len() != before {
            self.sort_and_rechunk();
            self.version = self.version.wrapping_add(1);
        }
    }

    /// 添加 MIDI 文档中指定音轨的音符
    fn add_midi_track_notes(&mut self, doc: &MidiDocument, track_idx: usize) {
        let track_idx_u16 = track_idx as u16;
        for note in doc.track_notes(track_idx) {
            self.notes.push(OnionNote::new(
                note.start_tick,
                note.end_tick(),
                note.key,
                track_idx_u16,
            ));
        }
    }

    /// 排序并重新分块
    ///
    /// 音符按 start_tick 排序，然后每 CHUNK_SIZE 个划分为一个 chunk。
    /// 每个 chunk 记录其时间范围和音符索引范围。
    fn sort_and_rechunk(&mut self) {
        self.notes.sort_by_key(|n| n.start_tick());

        self.chunks.clear();
        if self.notes.is_empty() {
            return;
        }

        let mut start = 0usize;
        while start < self.notes.len() {
            let end = (start + CHUNK_SIZE).min(self.notes.len());
            let tick_start = self.notes[start].start_tick();
            let tick_end = self.notes[start..end]
                .iter()
                .map(|n| n.end_tick())
                .max()
                .unwrap_or(tick_start);
            self.chunks.push(OnionNoteChunk {
                tick_start,
                tick_end,
                note_start: start,
                note_end: end,
            });
            start = end;
        }
    }
}

/// 从 `NoteInfo` 切片构建音符列表
///
/// 用于单元测试
#[must_use]
pub fn build_list_from_notes(notes: &[NoteInfo], track_idx: u16) -> OnionNoteList {
    let mut list = OnionNoteList::new();
    for note in notes {
        list.notes.push(OnionNote::new(
            note.start_tick,
            note.end_tick(),
            note.key,
            track_idx,
        ));
    }
    list.sort_and_rechunk();
    list.version = 1;
    list
}

#[cfg(test)]
mod tests;
