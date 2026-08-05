//! 访问器 —— 快捷 getter / setter 方法

use std::collections::HashSet;

use super::EditorData;
use lumino_note_core::note::Note;

impl EditorData {
    /// 标记 track_notes 已变化（递增版本号）
    ///
    /// 所有直接修改 `self.track_notes` 的地方都必须在操作后调用此方法，
    /// 否则 NoteWorker 快照缓存无法感知数据变化。
    ///
    /// 变化来源未知或影响全部音轨（`onion_dirty_tracks = None`），
    /// 洋葱皮会保守执行全量重建。调用方若能明确受影响音轨，
    /// 请使用 [`Self::mark_track_notes_changed_for`] 以获得增量豁免。
    #[inline]
    pub fn mark_track_notes_changed(&mut self) {
        self.mark_track_notes_changed_for(None);
    }

    /// 标记 track_notes 已变化，并记录明确受影响的音轨集合
    ///
    /// `tracks` 为本次操作实际修改的音轨 id 集合。当集合全部落在
    /// 洋葱皮跳过范围（当前音轨 / 静音音轨）时，可豁免全量重建上传。
    /// `None` 表示未知或影响全部音轨（保守语义，同 [`Self::mark_track_notes_changed`]）。
    ///
    /// 主音轨增量对账（2026-08-05）：当前音轨变化默认置 `note_delta_dirty = true`
    /// （未记录事件 → 渲染层全量兜底）。仅其他音轨变化不影响主音轨数据，
    /// 不置 dirty（洋葱皮编辑不牵连主音轨增量路径）。
    #[inline]
    pub fn mark_track_notes_changed_for(&mut self, tracks: Option<HashSet<usize>>) {
        self.onion_dirty_tracks = tracks;
        self.track_notes_gen = self.track_notes_gen.wrapping_add(1);
        match &self.onion_dirty_tracks {
            Some(t) if !t.contains(&self.current_track) => {}
            _ => self.note_delta_dirty = true,
        }
    }

    /// 标记当前音轨的 track_notes 已变化（热路径专用）
    ///
    /// 编辑操作绝大多数作用于当前音轨（拖动音符、增删改），而洋葱皮
    /// 不显示当前音轨——精确记录音轨 id 后，洋葱皮可豁免全量重建上传，
    /// 避免「拖动主音轨音符 → 每帧全量重传其他所有音轨」的冗余。
    #[inline]
    pub fn mark_current_track_changed(&mut self) {
        let current_track = self.current_track;
        self.mark_track_notes_changed_for(Some(HashSet::from([current_track])));
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

    /// 取走主音轨增量事件队列（UI 层每帧消费）
    #[inline]
    pub fn take_note_delta_events(
        &mut self,
    ) -> Vec<crate::editor_state::editor_data::NoteDeltaEvent> {
        std::mem::take(&mut self.note_delta_events)
    }

    /// 记录等长修改增量事件（整轨同步版）
    ///
    /// 将 `indices`（修改的 notes 索引，无序可重复）合并为连续区间
    /// `UpdateRange` 事件，随后整轨同步 `track_notes`（内部 mark，置 dirty）
    /// 并清除 dirty（事件已完整记录）。
    ///
    /// 供 EditorTransform（变速/翻转/移调/批量编辑）等整轨同步路径使用。
    pub fn record_update_ranges(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        self.push_update_range_events(indices);
        self.sync_track_notes();
        self.note_delta_dirty = false;
    }

    /// 记录等长修改增量事件（流式同步版，拖动热路径）
    ///
    /// 与 [`Self::record_update_ranges`] 相同的事件记录，但用
    /// `sync_track_notes_at_indices` 流式同步（避免整轨克隆，
    /// 1600W 音符拖动热路径）。供 `apply_drag_state_streaming` 使用。
    pub fn record_update_ranges_streamed(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        self.push_update_range_events(indices);
        self.sync_track_notes_at_indices(indices);
        self.note_delta_dirty = false;
    }

    /// 将升序去重后的索引合并为连续区间事件（纯数据操作，不同步）
    fn push_update_range_events(&mut self, indices: &[usize]) {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.is_empty() {
            return;
        }
        let mut start = sorted[0];
        let mut prev = sorted[0];
        for &i in &sorted[1..] {
            if i == prev + 1 {
                prev = i;
                continue;
            }
            self.push_update_range(start, prev);
            start = i;
            prev = i;
        }
        self.push_update_range(start, prev);
    }

    /// 推送单个连续区间事件（越界索引防御性过滤）
    fn push_update_range(&mut self, start: usize, end: usize) {
        let notes: Vec<Note> = (start..=end)
            .filter_map(|k| self.notes.get(k).cloned())
            .collect();
        if !notes.is_empty() {
            self.note_delta_events.push(
                crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange {
                    start_index: start,
                    notes,
                },
            );
        }
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
