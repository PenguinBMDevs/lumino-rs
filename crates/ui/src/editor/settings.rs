use super::CacheInvalidation;
use crate::constants::editor::{MAX_VISIBLE_KEY_COUNT, MIN_VISIBLE_KEY_COUNT};
use lumino_core::storage::config::EraserBehavior;

impl super::Editor {
    // 键盘设置 — 全部委托到 editor_state

    pub fn set_visible_key_count(&mut self, count: u16) {
        let old_count = self.editor_state.view.visible_key_count;
        let canvas_height = self.editor_state.canvas.size.y;

        self.editor_state.set_visible_key_count(
            count,
            MIN_VISIBLE_KEY_COUNT,
            MAX_VISIBLE_KEY_COUNT,
            canvas_height,
        );

        // 向上拓展：高键号在上方，扩展键（128-255）应出现在原有键位之上
        // 原有最高键(127)的 world_y 从 0 变为 added_keys*zoom_y
        // 需要同步增加 scroll_y 使原有可见区域保持不变
        if count > old_count {
            let added_keys = (count - old_count) as f32;
            self.editor_state.view.scroll_y += added_keys * self.editor_state.view.zoom_y;
            // 重新钳位到有效范围
            let vh = (canvas_height - self.editor_state.view.ruler_height).max(0.0);
            let max_sy = (self.editor_state.max_scroll.y - vh).max(0.0);
            self.editor_state.view.scroll_y = self.editor_state.view.scroll_y.clamp(0.0, max_sy);
        }

        // 键盘和网格缓存都需要刷新：key_count 变了，键盘绘制和网格线都要重建
        self.invalidate_caches(super::CacheInvalidation::ALL);
    }

    pub fn visible_key_count(&self) -> u16 {
        self.editor_state.view.visible_key_count
    }

    pub fn set_keyboard_width(&mut self, width: f32) {
        self.editor_state.set_keyboard_width(width);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    pub fn keyboard_width(&self) -> f32 {
        self.editor_state.view.keyboard_width
    }

    // 音符设置

    pub fn set_snap_precision(&mut self, precision: f32) {
        self.editor_state.set_snap_precision(precision);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    pub fn snap_precision(&self) -> f32 {
        self.editor_state.view.snap_precision
    }

    pub fn set_default_note_length(&mut self, length: f32) {
        self.editor_state.set_default_note_length(length);
        self.invalidate_caches(CacheInvalidation::GRID);
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
