use super::{EditState, Editor, HitType, SelectionHitType};
use iced_core::Point;
use lumino_core::editor_state::hit_test;
use lumino_ui_constants::editor::NOTE_EDGE_THRESHOLD_PX;

use std::collections::HashSet;

/// 收集当前 ghost 偏移影响的所有音符索引，按索引降序返回
///
/// 来源包括：pending_drag_state（异步提交完成前）和当前编辑状态中的 drag_state。
/// 降序是为了 hit test 时优先匹配视觉上靠上的音符。
fn collect_ghost_indices(
    edit_state: &EditState,
    pending: &Option<lumino_core::DragState>,
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
        //    这些音符视觉上已移动，但 data.notes 和空间索引仍是旧位置，
        //    必须按 ghost 位置命中，否则触控判定区域会停留在原地。
        //    从后向前遍历，优先命中视觉上靠上的音符（与原逻辑一致）。
        let ghost_indices = collect_ghost_indices(edit_state, pending);
        let ghost_set: HashSet<usize> = ghost_indices.iter().copied().collect();
        for &i in &ghost_indices {
            let Some(note) = self.editor_state.data.notes.get(i) else {
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
            let note = self.editor_state.data.notes.get(best_idx)?;
            Self::note_hit_type(tick, note.tick, note.length, edge_threshold)
                .map(|hit_type| (best_idx, hit_type))
        } else {
            // 小数据量线性扫描：排除 ghost 音符后，剩余音符均无 ghost 偏移，
            // 直接使用原始 tick/key 判定，避免重复调用 ghost_delta_for_index。
            let mut best_idx = None;
            for (i, note) in self.editor_state.data.notes.iter().enumerate().rev() {
                if ghost_set.contains(&i) {
                    continue;
                }
                if note.key == key
                    && tick >= note.tick
                    && tick <= note.tick + note.length
                    && best_idx.is_none_or(|b| i > b)
                {
                    best_idx = Some(i);
                }
            }
            let i = best_idx?;
            let note = self.editor_state.data.notes.get(i)?;
            Self::note_hit_type(tick, note.tick, note.length, edge_threshold)
                .map(|hit_type| (i, hit_type))
        }
    }

    /// 根据点击位置相对音符起止点的距离判定命中类型
    fn note_hit_type(
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

    pub fn delete_note_by_index(&mut self, index: usize) {
        // Capture note info before deletion for sync event
        let note_info = self.editor_state.data.notes.get(index).map(|n| {
            (
                n.tick,
                n.key,
                n.length,
                n.velocity,
                n.channel,
                self.editor_state.data.current_track,
            )
        });

        self.editor_state.data.delete_note_by_index(index);
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // Emit sync event for deletion
        if let Some((tick, key, length, velocity, channel, track_idx)) = note_info {
            lumino_event::emit(lumino_event::Event::Window(
                lumino_event::window::Event::local_note_deleted(
                    tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }

    pub fn delete_note_at(&mut self, pos: Point) -> bool {
        if let Some((index, _)) = self.hit_test_note(pos) {
            self.delete_note_by_index(index);
            true
        } else {
            false
        }
    }

    pub fn is_note_selected(&self, index: usize) -> bool {
        self.editor_state
            .interaction
            .selected_notes
            .contains(&index)
    }

    pub fn selected_notes_count(&self) -> usize {
        self.editor_state.interaction.selected_notes.len()
    }

    pub fn clear_selection(&mut self) {
        self.editor_state.interaction.selected_notes.clear();
    }

    pub fn delete_selected_notes(&mut self) {
        let indices = self.editor_state.interaction.selected_notes.clone();

        // Capture note info before deletion for sync events
        let deleted_notes: Vec<_> = indices
            .iter()
            .filter_map(|&i| {
                self.editor_state.data.notes.get(i).map(|n| {
                    (
                        n.tick,
                        n.key,
                        n.length,
                        n.velocity,
                        n.channel,
                        self.editor_state.data.current_track,
                    )
                })
            })
            .collect();

        self.editor_state.data.delete_selected_notes(&indices);
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // Emit sync events for each deleted note
        for (tick, key, length, velocity, channel, track_idx) in deleted_notes {
            lumino_event::emit(lumino_event::Event::Window(
                lumino_event::window::Event::local_note_deleted(
                    tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }

    pub fn select_all_notes(&mut self) {
        self.editor_state.interaction.selected_notes = self.editor_state.data.select_all_notes();
    }

    pub fn get_selection_box_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        crate::puffin_profiler::get_selection_box_bounds();
        let notes = &self.editor_state.data.notes;
        let view = &self.editor_state.view;
        let selected = &self.editor_state.interaction.selected_notes;
        let max_key = view.visible_key_count.saturating_sub(1);
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;

        if selected.is_empty() {
            return None;
        }

        // 性能优化：先判断是否需要 ghost delta，避免在循环中每元素调用。
        let needs_ghost = pending.is_some()
            || matches!(
                edit_state,
                EditState::Dragging { .. } | EditState::DraggingSelection { .. }
            );

        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        let mut any = false;

        if needs_ghost {
            for &i in selected.iter() {
                let Some(n) = notes.get(i) else {
                    continue;
                };
                any = true;
                let (tick, key) = if let Some((dt, dk)) =
                    crate::rendering::ghost_delta_for_index(i, pending, edit_state)
                {
                    (
                        (n.tick + dt as f32).max(0.0),
                        (n.key as i32 + dk as i32).clamp(0, max_key as i32) as u16,
                    )
                } else {
                    (n.tick, n.key)
                };
                min_t = min_t.min(tick);
                max_te = max_te.max(tick + n.length);
                max_k = max_k.max(key);
                min_k = min_k.min(key);
            }
        } else {
            for &i in selected.iter() {
                let Some(n) = notes.get(i) else {
                    continue;
                };
                any = true;
                min_t = min_t.min(n.tick);
                max_te = max_te.max(n.tick + n.length);
                max_k = max_k.max(n.key);
                min_k = min_k.min(n.key);
            }
        }
        if !any {
            return None;
        }
        Some((
            view.tick_to_x(min_t),
            view.tick_to_x(max_te),
            view.key_to_y(max_k),
            view.key_to_y(min_k) + view.zoom_y,
        ))
    }

    pub fn hit_test_selection_box(&self, pos: Point) -> Option<SelectionHitType> {
        crate::puffin_profiler::hit_test_selection_box();
        let bounds = self.get_selection_box_bounds()?;
        hit_test::hit_test_selection_box(bounds, (pos.x, pos.y))
    }
}
