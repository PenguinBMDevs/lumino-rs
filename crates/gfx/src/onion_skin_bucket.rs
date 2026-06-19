//! 洋葱皮音符列表 — 参考 Wasabi 瀑布流实现的简化数据结构
//!
//! 替换原有的按 key 分桶缓存（OnionSkinBucket），改用简单的扁平 Vec。
//! 核心变化：
//! - 移除 256 个 key 的 per-key 分桶和排序
//! - 移除 render cursor 和 collect_visible_with_cursor
//! - 扁平存储，由 GPU vertex shader 处理可见性裁剪（GPU 原生 clip）
//! - 对应 wasabi 的 NoteList 概念，方向旋转为钢琴卷帘（X=time, Y=pitch）

use lumino_midi_loader::{MidiDocument, NoteInfo};

use crate::OnionNote;

/// 洋葱皮音符列表 — 扁平存储所有洋葱皮音符
///
/// 设计思路（源自 Wasabi）：
/// - 所有音符按 track_idx 分组的扁平数组
/// - 由 GPU vertex shader 计算 NDC 坐标，超出 [-1, 1] 的自动被 GPU clip
/// - 无需 CPU 端 per-key 排序/二分查找/可见性判断
/// - 相比旧 OnionSkinBucket：256 buckets × sorted + collect_visible_with_cursor 全部移除
#[derive(Debug, Clone)]
pub struct OnionNoteList {
    /// 扁平音符数组
    notes: Vec<OnionNote>,
    /// 数据版本号，每次 rebuild/update 后递增
    version: u64,
}

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

    /// 获取音符切片
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[OnionNote] {
        &self.notes
    }

    /// 清空所有音符
    pub fn clear(&mut self) {
        self.notes.clear();
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

        self.version = self.version.wrapping_add(1);
    }

    /// 添加/更新用户编辑音轨的音符
    ///
    /// 先移除该音轨已有的音符，再追加新音符。
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

        self.version = self.version.wrapping_add(1);
    }

    /// 从列表中移除指定音轨的所有音符
    pub fn remove_track(&mut self, track_idx: u16) {
        let before = self.notes.len();
        self.notes.retain(|n| n.track_idx() != track_idx);
        if self.notes.len() != before {
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
    list.version = 1;
    list
}

#[cfg(test)]
mod tests;
