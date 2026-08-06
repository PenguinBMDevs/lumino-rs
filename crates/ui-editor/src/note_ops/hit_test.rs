//! 音符命中检测：hit_test_note + note_hit_type + ghost 索引收集
//!
//! **性能关键**：1000W 音符场景下，原 O(N) 全量扫描 ~168ms/帧。
//! 改用空间索引 `update_query` 剪枝 + key 二分查找，降到 O(log N + K)。
//! 空间索引未建（None）时 fallback 到 O(N) 扫描保证准确性。

use std::collections::HashSet;

use iced_core::Point;

use super::super::{EditState, Editor, HitType};
use lumino_ui_core::constants::editor::NOTE_EDGE_THRESHOLD_PX;

/// 收集当前 ghost 偏移影响的所有音符索引，按索引降序返回
///
/// 来源包括：pending_drag_state（异步提交完成前）和当前编辑状态中的 drag_state。
/// 降序是为了 hit test 时优先匹配视觉上靠上的音符。
fn collect_ghost_indices(
    edit_state: &EditState,
    pending: &Option<lumino_editor_state::DragState>,
) -> Vec<usize> {
    let mut set = HashSet::new();
    if let Some(pending) = pending {
        for i in pending.selected_indices() {
            set.insert(i);
        }
    }
    match edit_state {
        EditState::Dragging { drag_state, .. } | EditState::DraggingSelection { drag_state } => {
            for i in drag_state.selected_indices() {
                set.insert(i);
            }
        }
        _ => {}
    }
    let mut indices: Vec<usize> = set.into_iter().collect();
    indices.sort_unstable_by(|a, b| b.cmp(a));
    indices
}

impl Editor {
    /// 检测坐标是否落在某个音符上
    ///
    /// **性能关键**：1000W 音符场景下，原 O(N) 全量扫描 ~168ms/帧。
    /// 改用空间索引 `update_query` 剪枝 + key 二分查找，降到 O(log N + K)。
    ///
    /// 空间索引未建（None）时 fallback 到 O(N) 扫描保证准确性。
    /// Resizing 期间空间索引可能滞后一帧（dirty 未重建），hover 位置略旧可接受；
    /// pressed 通常在 Idle 状态触发，空间索引是最新的，准确性有保证。
    pub fn hit_test_note(&self, pos: Point) -> Option<(usize, HitType)> {
        // 按需重建空间索引；小数据量时直接走线性扫描，避免百毫秒级建树开销。
        self.ensure_spatial_index();

        let view = &self.editor_state.view;
        let tick = view.x_to_tick(pos.x);
        let key = view.y_to_key(pos.y);
        let edge_threshold = NOTE_EDGE_THRESHOLD_PX / view.zoom_x;
        let max_key = view.visible_key_count.saturating_sub(1);
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;

        // 1. 先检查 ghost 偏移影响的音符（pending / 当前 drag）。
        //    这些音符视觉上已移动，但 document 和空间索引仍是旧位置，
        //    必须按 ghost 位置命中，否则触控判定区域会停留在原地。
        //    从后向前遍历，优先命中视觉上靠上的音符（与原逻辑一致）。
        let ghost_indices = collect_ghost_indices(edit_state, pending);
        let ghost_set: HashSet<usize> = ghost_indices.iter().copied().collect();
        for &i in &ghost_indices {
            let Some(note) = self.editor_state.data.get_note_view(i) else {
                continue;
            };
            let Some(delta) = crate::rendering::ghost_delta_for_index(i, pending, edit_state)
            else {
                continue;
            };
            let ghost_tick = (note.tick + delta.0 as f32).max(0.0);
            let ghost_key = (note.key as i32 + delta.1 as i32).clamp(0, max_key as i32) as u16;
            if ghost_key == key
                && let Some(hit_type) =
                    Self::note_hit_type(tick, ghost_tick, note.length, edge_threshold)
            {
                return Some((i, hit_type));
            }
        }

        // 2. 非 ghost 音符命中判定。
        //    - 大数据量：使用空间索引 O(log N + K)。
        //    - 小数据量：线性扫描 O(N)，避免构建索引的固定开销。
        //    排除 ghost 受影响的音符，避免视觉上已移走的音符仍在原位置被命中。
        if let Some(index) = self.spatial.note_index.borrow().as_ref() {
            let mut buf = Vec::new();
            index.update_query(tick, tick, key, key, &mut buf);
            let best_idx = *buf.iter().filter(|&&i| !ghost_set.contains(&i)).max()?;
            let note = self.editor_state.data.get_note_view(best_idx)?;
            Self::note_hit_type(tick, note.tick, note.length, edge_threshold)
                .map(|hit_type| (best_idx, hit_type))
        } else {
            // 小数据量线性扫描：排除 ghost 音符后，剩余音符均无 ghost 偏移，
            // 直接使用原始 tick/key 判定，避免重复调用 ghost_delta_for_index。
            //
            // 此 fallback 路径仅在 spatial.note_index 为 None 时进入（小数据量
            // < SPATIAL_INDEX_BUILD_THRESHOLD = 50000），此时 NoteStore 也未启用
            // （阈值 10000），直接走 document 切片 iter 最快——无需构造 NoteView。
            let mut best_idx = None;
            // 2026-08 单一权威源：current_track_notes 返回分块容器（u32 tick/u8 key）
            // 反序遍历：分块容器不支持 DoubleEndedIterator，改用全局索引倒序
            let track_notes = self.editor_state.data.current_track_notes();
            for i in (0..track_notes.len()).rev() {
                let Some(note) = track_notes.get(i) else {
                    break;
                };
                if ghost_set.contains(&i) {
                    continue;
                }
                if note.key as u16 == key
                    && tick >= note.start_tick as f32
                    && tick <= note.end_tick as f32
                    && best_idx.is_none_or(|b| i > b)
                {
                    best_idx = Some(i);
                }
            }
            let i = best_idx?;
            let note = self.editor_state.data.get_note_view(i)?;
            Self::note_hit_type(tick, note.tick, note.length, edge_threshold)
                .map(|hit_type| (i, hit_type))
        }
    }

    /// 根据点击位置相对音符起止点的距离判定命中类型
    pub(crate) fn note_hit_type(
        tick: f32,
        note_tick: f32,
        length: f32,
        edge_threshold: f32,
    ) -> Option<HitType> {
        let start_delta = (tick - note_tick).abs();
        let end_delta = (tick - (note_tick + length)).abs();
        if end_delta < edge_threshold {
            Some(HitType::End)
        } else if start_delta < edge_threshold {
            Some(HitType::Start)
        } else if tick >= note_tick && tick <= note_tick + length {
            Some(HitType::Middle)
        } else {
            None
        }
    }
}
