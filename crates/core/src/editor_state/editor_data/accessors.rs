//! 访问器 —— 快捷 getter / setter 方法

use super::EditorData;
use crate::note::Note;

impl EditorData {
    /// 标记 track_notes 已变化（递增版本号）
    ///
    /// 所有直接修改 `self.track_notes` 的地方都必须在操作后调用此方法，
    /// 否则 NoteWorker 快照缓存无法感知数据变化。
    #[inline]
    pub fn mark_track_notes_changed(&mut self) {
        self.track_notes_gen = self.track_notes_gen.wrapping_add(1);
    }

    /// 返回文档音轨索引对应的视觉位置
    ///
    /// 在 ChannelGrouped 模式下，侧边栏音轨按 channel 分组，视觉位置与
    /// 文档音轨索引不一定相等。此方法用于将 track_notes 的键（文档音轨索引）
    /// 映射到视觉位置，以便与 ArrangeSelection 中的 track 范围进行比较。
    ///
    /// 如果音轨不在映射中，返回 `None`（此时回退到 `track_idx` 本身作为视觉位置）。
    pub fn visual_position_of(&self, track_id: usize) -> Option<usize> {
        self.track_visual_order
            .iter()
            .position(|&id| id == track_id)
    }

    /// 获取当前轨道音符集合的零拷贝引用。
    ///
    /// 优先从 `track_notes` 中读取当前选中的音轨；若不存在则返回空 `Vector`，
    /// 避免构造第二份拷贝。
    pub fn current_track_notes(&self) -> &im::Vector<Note> {
        static EMPTY: std::sync::OnceLock<im::Vector<Note>> = std::sync::OnceLock::new();
        self.track_notes
            .get(&self.current_track)
            .unwrap_or_else(|| EMPTY.get_or_init(im::Vector::new))
    }
}
