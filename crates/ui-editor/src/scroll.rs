use super::CacheInvalidation;
use lumino_core::editor_state::viewport::Viewport;
use lumino_ui_constants::editor::zoom::{MAX_ZOOM_X, MAX_ZOOM_Y, MIN_ZOOM_X, MIN_ZOOM_Y};

impl super::Editor {
    // 滚动控制 — 直接通过 viewport 模块操作

    pub fn set_max_scroll_x(&mut self, max_scroll: f32) {
        self.editor_state.max_scroll.0 = max_scroll;
    }

    pub fn set_max_scroll_y(&mut self, max_scroll: f32) {
        self.editor_state.max_scroll.1 = max_scroll;
    }

    pub fn scroll_x(&self) -> f32 {
        self.editor_state.view.scroll_x
    }

    pub fn scroll_y(&self) -> f32 {
        self.editor_state.view.scroll_y
    }

    /// 获取滚动位置 (x, y)
    pub fn scroll(&self) -> (f32, f32) {
        (
            self.editor_state.view.scroll_x,
            self.editor_state.view.scroll_y,
        )
    }

    /// 获取缩放 (x, y)
    pub fn zoom(&self) -> (f32, f32) {
        (self.editor_state.view.zoom_x, self.editor_state.view.zoom_y)
    }

    pub fn zoom_x(&self) -> f32 {
        self.editor_state.view.zoom_x
    }

    pub fn zoom_y(&self) -> f32 {
        self.editor_state.view.zoom_y
    }

    pub fn set_scroll_x(&mut self, scroll_x: f32) {
        let keyboard_width = self.editor_state.view.keyboard_width;
        let canvas_width = self.editor_state.canvas.size_x;
        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_scroll_x(scroll_x, keyboard_width, canvas_width);
        self.invalidate_caches(CacheInvalidation::RULER);
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        let canvas_height = self.editor_state.canvas.size_y;
        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_scroll_y(scroll_y, canvas_height);
        self.invalidate_caches(CacheInvalidation::KEYBOARD);
    }

    pub fn set_zoom_x(&mut self, zoom_x: f32, fixed_ratio: f32) {
        let keyboard_width = self.editor_state.view.keyboard_width;
        let canvas_width = self.editor_state.canvas.size_x;
        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_zoom_x(
            zoom_x,
            fixed_ratio,
            keyboard_width,
            canvas_width,
            MIN_ZOOM_X,
            MAX_ZOOM_X,
        );
        self.invalidate_caches(CacheInvalidation::RULER);
    }

    pub fn set_zoom_y(&mut self, zoom_y: f32, fixed_ratio: f32) {
        let canvas_height = self.editor_state.canvas.size_y;
        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_zoom_y(zoom_y, fixed_ratio, canvas_height, MIN_ZOOM_Y, MAX_ZOOM_Y);
        self.invalidate_caches(CacheInvalidation::KEYBOARD);
    }
}
