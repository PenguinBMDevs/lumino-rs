//! 音符与音轨读取访问器（查询 / 反查 / 增量事件取出）
//!
//! 由 `accessors.rs` 拆分而来。

use super::*;

impl EditorData {
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

    /// 返回视觉位置对应的文档音轨索引（与 [`Self::visual_position_of`] 互逆）
    ///
    /// 侧边栏顺序即视觉顺序（拖动排序后 `track_visual_order` 同步更新）。
    /// 音轨不在映射中时回退到恒等映射（视觉位置即文档索引），
    /// 保证未初始化/部分同步状态下不会越界访问。
    #[inline]
    pub fn document_track_at(&self, visual_pos: usize) -> usize {
        self.track_visual_order
            .get(visual_pos)
            .copied()
            .unwrap_or(visual_pos)
    }

    /// 取走主音轨增量事件队列（UI 层每帧消费）
    #[inline]
    pub fn take_note_delta_events(
        &mut self,
    ) -> Vec<crate::editor_state::editor_data::NoteDeltaEvent> {
        std::mem::take(&mut self.note_delta_events)
    }

    // ── 音符读取（document 唯一权威） ─────────────────────────

    /// 获取当前轨道音符的分块引用（零拷贝，直接借自 document）
    ///
    /// 无 document 或音轨不存在时返回空容器引用。
    #[inline]
    pub fn current_track_notes(&self) -> &lumino_midi_model::ChunkedList<NoteEvent> {
        self.track_notes(self.current_track)
    }

    /// 获取指定音轨音符的分块引用（零拷贝，直接借自 document）
    #[inline]
    pub fn track_notes(&self, track_id: usize) -> &lumino_midi_model::ChunkedList<NoteEvent> {
        static EMPTY: lumino_midi_model::ChunkedList<NoteEvent> =
            lumino_midi_model::ChunkedList::EMPTY;
        self.document
            .as_ref()
            .map(|doc| doc.track_notes(track_id))
            .unwrap_or(&EMPTY)
    }

    /// 当前轨道音符数量（无 document 时为 0）
    #[inline]
    pub fn current_track_note_count(&self) -> usize {
        self.document
            .as_ref()
            .map(|doc| doc.track_note_count(self.current_track as u16) as usize)
            .unwrap_or(0)
    }

    /// 按 `(音轨, tick, key)` 反查音符全局唯一 ID（协作按 id 同步时取回真实 id）。
    ///
    /// tick/key 带容差匹配（≤1 tick、key 全相等），避免浮点/整型换算误差；
    /// 命中多个时取 tick 最近者。无 document 或未命中返回 `None`。
    pub fn note_id_at(&self, track_id: usize, tick: f32, key: u16) -> Option<u64> {
        let notes = self.track_notes(track_id);
        let mut best: Option<(f32, u64)> = None;
        for n in notes.iter() {
            let dt = (n.start_tick as f32 - tick).abs();
            let dk = (n.key as i32 - key as i32).abs();
            if dt <= 1.0 && dk == 0 {
                let score = dt;
                if best.is_none_or(|b| score < b.0) {
                    best = Some((score, n.id));
                }
            }
        }
        best.map(|b| b.1)
    }
}
