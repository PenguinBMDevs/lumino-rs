use crate::constants::editor::{MAX_VISIBLE_KEY_COUNT, MIN_VISIBLE_KEY_COUNT};

impl super::Editor {
    // 键盘设置

    pub fn set_visible_key_count(&mut self, count: u16) {
        let clamped_count = count.clamp(MIN_VISIBLE_KEY_COUNT, MAX_VISIBLE_KEY_COUNT);
        self.state.visible_key_count = clamped_count;
        self.max_scroll_y = clamped_count as f32 * self.state.zoom_y;
        if self.state.scroll_y > self.max_scroll_y {
            self.state.scroll_y = self.max_scroll_y;
        }
        self.grid_cache.clear();
    }

    pub fn visible_key_count(&self) -> u16 {
        self.state.visible_key_count
    }

    pub fn set_keyboard_width(&mut self, width: f32) {
        self.state.keyboard_width = width.max(0.0);
        self.grid_cache.clear();
    }

    pub fn keyboard_width(&self) -> f32 {
        self.state.keyboard_width
    }

    // 音符设置

    pub fn set_snap_precision(&mut self, precision: f32) {
        self.state.snap_precision = precision.max(1.0);
        self.grid_cache.clear();
    }

    pub fn snap_precision(&self) -> f32 {
        self.state.snap_precision
    }

    pub fn set_default_note_length(&mut self, length: f32) {
        self.state.default_note_length = length.max(1.0);
        self.grid_cache.clear();
    }

    pub fn default_note_length(&self) -> f32 {
        self.state.default_note_length
    }
}
