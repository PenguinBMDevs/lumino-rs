//! 音符翻转操作模块

use super::Editor;
use crate::toolbar::FlipHorizontalMode;

impl Editor {
    pub fn flip_selected_notes_vertical(&mut self) -> usize {
        let selected = self.editor_state.interaction.selected_notes.clone();
        let max_key_index = (self.editor_state.view.visible_key_count - 1) as f32;
        let result = self.editor_state.data.flip_vertical(&selected, max_key_index);
        if result > 0 {
            self.editor_state.interaction.selected_notes.clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }

    pub fn flip_selected_notes_horizontal(&mut self, mode: FlipHorizontalMode) -> usize {
        let selected = self.editor_state.interaction.selected_notes.clone();
        let notes = &self.editor_state.data.notes;
        let indices: Vec<usize> = selected.iter().copied().collect();
        if indices.is_empty() { return 0; }
        let mut min_tick = f32::INFINITY;
        let mut max_tick_end = f32::NEG_INFINITY;
        for &i in &indices {
            if let Some(n) = notes.get(i) {
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
            self.editor_state.interaction.selected_notes.clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }
}
