//! 音符翻转操作模块

use super::Editor;
use lumino_editor_state::EditorTransform;
use std::collections::HashSet;

use lumino_ui_core::toolbar_event::FlipHorizontalMode;

impl Editor {
    pub fn flip_selected_notes_vertical(&mut self) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let max_key_index = (self.editor_state.view.visible_key_count - 1) as f32;
        let result = self
            .editor_state
            .data
            .flip_vertical(&selected, max_key_index);
        if result > 0 {
            self.selection_clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }

    pub fn flip_selected_notes_horizontal(&mut self, mode: FlipHorizontalMode) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let indices: Vec<usize> = selected.iter().copied().collect();
        if indices.is_empty() {
            return 0;
        }
        // 2026-08 单一权威源：经 get_note_view 读取（NoteView: tick f32/length f32）
        let data = &self.editor_state.data;
        let mut min_tick = f32::INFINITY;
        let mut max_tick_end = f32::NEG_INFINITY;
        for &i in &indices {
            if let Some(n) = data.get_note_view(i) {
                min_tick = min_tick.min(n.tick);
                max_tick_end = max_tick_end.max(n.tick + n.length);
            }
        }
        let axis_tick = match mode {
            FlipHorizontalMode::Center => (min_tick + max_tick_end) / 2.0,
            FlipHorizontalMode::Left => min_tick,
            FlipHorizontalMode::Right => max_tick_end,
        };
        let result = self.editor_state.data.flip_horizontal(&selected, axis_tick);
        if result > 0 {
            self.selection_clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }
}
