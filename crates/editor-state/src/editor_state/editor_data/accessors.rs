//! 访问器 —— 快捷 getter / setter 方法

use super::EditorData;
use lumino_note_core::note::Note;

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
    /// 侧边栏音轨按原始序号排列，视觉位置与文档音轨索引一致（恒等映射）。
    /// 此方法保留供 arrangement 操作统一使用，便于未来支持拖动排序等变化。
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
