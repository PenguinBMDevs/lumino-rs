use super::{Editor, HitType, SelectionHitType};
use iced_core::Point;
use lumino_core::editor_state::hit_test;
use lumino_event;
use lumino_ui_constants::editor::NOTE_EDGE_THRESHOLD_PX;

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
        let tick = view.x_to_tick(pos.x);
        let key = view.y_to_key(pos.y);
        let edge_threshold = NOTE_EDGE_THRESHOLD_PX / view.zoom_x;

        // 优先用空间索引（O(log N + K)），fallback 到 O(N) 扫描
        let candidates: Vec<usize> = if let Some(index) = self.spatial.note_index.borrow().as_ref()
        {
            let mut buf = Vec::new();
            // 查询包含点 (tick, key) 的音符：
            // - tick 范围 [tick, tick]：剪枝 node.tick_max < tick || node.tick_min > tick
            // - key 范围 [key, key]：partition_point 二分查找
            // - 过滤 n.tick + n.length >= tick && n.tick <= tick（包含该点）
            index.update_query(tick, tick, key, key, &mut buf);
            buf
        } else {
            // 空间索引未建（首次或刚清空），fallback 到 O(N) 扫描
            self.editor_state
                .data
                .notes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.key == key && tick >= n.tick && tick <= n.tick + n.length)
                .map(|(i, _)| i)
                .collect()
        };

        if candidates.is_empty() {
            return None;
        }

        // 选 index 最大的（视觉最上层，等价于原 .rev() 的第一个匹配）
        let best_idx = *candidates.iter().max()?;
        let note = self.editor_state.data.notes.get(best_idx)?;
        let start_delta = (tick - note.tick).abs();
        let end_delta = (tick - (note.tick + note.length)).abs();

        let hit_type = if end_delta < edge_threshold {
            HitType::End
        } else if start_delta < edge_threshold {
            HitType::Start
        } else {
            HitType::Middle
        };
        Some((best_idx, hit_type))
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

    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        self.editor_state.get_notes_in_selection_box(
            start_tick,
            start_key,
            current_tick,
            current_key,
        )
    }

    pub(super) fn delete_notes_in_selection_box(
        &mut self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) {
        let indices = self.editor_state.get_notes_in_selection_box(
            start_tick,
            start_key,
            current_tick,
            current_key,
        );
        if indices.is_empty() {
            return;
        }

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

        let set: std::collections::HashSet<usize> = indices.into_iter().collect();
        self.editor_state.data.delete_selected_notes(&set);
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
        hit_test::get_selection_box_bounds(
            &self.editor_state.data.notes,
            &self.editor_state.view,
            &self.editor_state.interaction.selected_notes,
        )
    }

    pub fn hit_test_selection_box(&self, pos: Point) -> Option<SelectionHitType> {
        let bounds = self.get_selection_box_bounds()?;
        hit_test::hit_test_selection_box(bounds, (pos.x, pos.y))
    }
}
