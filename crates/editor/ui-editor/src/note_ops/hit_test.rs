//! 音符命中检测：hit_test_note + note_hit_type + ghost 索引收集
//!
//! **性能关键**：1000W 音符场景下，原 O(N) 全量扫描 ~168ms/帧。
//! 改用空间索引 `update_query` 剪枝 + key 二分查找，降到 O(log N + K)。
//!
//! 2026-08-06 性能修复：空间索引在 1600W 下每次编辑需 O(N log N) 全量重建
//! （collect NoteRef + sort + 递归建树，2-4s）——用户「编辑中间插入 4s」的主因。
//! 本模块改为「ChunkedList 窗口扫描」：`window_range` 块级二分框出命中点
//! tick 邻域（含 lookback 跨入长音符），窗口内过滤 key/start/end。O(log N + K)
//! 与总音符量无关，且**无需建任何索引**。空间索引（NoteSpatialIndex）不再为
//! 命中路径服务，作为冗余层退役。

use std::collections::HashSet;

use iced_core::Point;

use super::super::{EditState, Editor, HitType};
use lumino_ui_core::constants::editor::NOTE_EDGE_THRESHOLD_PX;

/// 命中查询 lookback 上界：向命中点左侧回溯的 tick 跨度（覆盖「跨入」长音符）
const HIT_WINDOW_LOOKBACK: u32 = 1_000_000;

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
        let view = &self.editor_state.view;
        let (tick, key) = if self.editor_state.is_vertical_roll {
            (self.pos_to_tick(pos), self.pos_to_key(pos))
        } else {
            (view.x_to_tick(pos.x), view.y_to_key(pos.y))
        };
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

        // 2. 非 ghost 音符命中判定：ChunkedList 窗口扫描。
        //    块级二分框出命中点 tick 邻域（起点 ≤ 命中点，含 lookback 跨入），
        //    窗口内过滤 key/start/end——O(log 块数 + 窗口长度)，免空间索引重建。
        //    排除 ghost 受影响的音符，避免视觉上已移走的音符仍在原位置被命中。
        let tick_u32 = tick.max(0.0) as u32;
        let (lo, hi) = self.editor_state.data.current_track_notes().window_range(
            tick_u32,
            tick_u32 + 1,
            HIT_WINDOW_LOOKBACK,
        );
        let mut best_idx = None;
        for (i, note) in self
            .editor_state
            .data
            .current_track_notes()
            .iter_window(lo, hi)
        {
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
