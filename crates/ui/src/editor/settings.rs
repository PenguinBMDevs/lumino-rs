use crate::constants::editor::{MAX_VISIBLE_KEY_COUNT, MIN_VISIBLE_KEY_COUNT};
use lumino_core::storage::config::EraserBehavior;

impl super::Editor {
    // 键盘设置 — 全部委托到 editor_state

    pub fn set_visible_key_count(&mut self, count: u16) {
        let canvas_height = self.editor_state.canvas.size.y;
        // editor_state.set_visible_key_count 已包含 clamp 和 scroll_y 修正逻辑
        self.editor_state.set_visible_key_count(count, MIN_VISIBLE_KEY_COUNT, MAX_VISIBLE_KEY_COUNT, canvas_height);
        self.grid_cache.clear();
    }

    pub fn visible_key_count(&self) -> u16 {
        self.editor_state.view.visible_key_count
    }

    pub fn set_keyboard_width(&mut self, width: f32) {
        self.editor_state.set_keyboard_width(width);
        self.grid_cache.clear();
    }

    pub fn keyboard_width(&self) -> f32 {
        self.editor_state.view.keyboard_width
    }

    // 音符设置

    pub fn set_snap_precision(&mut self, precision: f32) {
        self.editor_state.set_snap_precision(precision);
        self.grid_cache.clear();
    }

    pub fn snap_precision(&self) -> f32 {
        self.editor_state.view.snap_precision
    }

    pub fn set_default_note_length(&mut self, length: f32) {
        self.editor_state.set_default_note_length(length);
        self.grid_cache.clear();
    }

    pub fn default_note_length(&self) -> f32 {
        self.editor_state.view.default_note_length
    }

    // 橡皮擦设置

    pub fn set_eraser_behavior(&mut self, behavior: EraserBehavior) {
        self.editor_state.set_eraser_behavior(behavior);
    }

    pub fn eraser_behavior(&self) -> EraserBehavior {
        self.editor_state.view.eraser_behavior
    }
}
