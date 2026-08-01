use super::CacheInvalidation;
use lumino_editor_state::editor_state::viewport::Viewport;
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
        let visible_key_count = self.editor_state.view.visible_key_count;

        // 动态最小缩放：确保内容填满视口，防止键盘无限缩小超出渲染范围
        // 与 CanvasBoundsChanged 处理器的空区填充逻辑保持一致
        let ruler_height = self.editor_state.view.ruler_height;
        let vh = (canvas_height - ruler_height).max(0.0);
        let dynamic_min_zoom = if visible_key_count > 0 && vh > 0.0 {
            (vh / visible_key_count as f32).clamp(MIN_ZOOM_Y, MAX_ZOOM_Y)
        } else {
            MIN_ZOOM_Y
        };

        // 动态最大缩放：防止键盘键高超过视口可接受范围
        // 限制总内容高度不超过视口高度的 MAX_SCROLLABLE_PAGES 倍，
        // 128/256 键模式自动适配：键数越多，最大缩放越小
        const MAX_SCROLLABLE_PAGES: f32 = 16.0;
        let dynamic_max_zoom = if visible_key_count > 0 && canvas_height > 0.0 {
            MAX_ZOOM_Y
                .min(canvas_height * MAX_SCROLLABLE_PAGES / visible_key_count as f32)
                .max(MIN_ZOOM_Y)
        } else {
            MAX_ZOOM_Y
        };

        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_zoom_y(
            zoom_y,
            fixed_ratio,
            canvas_height,
            dynamic_min_zoom,
            dynamic_max_zoom,
        );
        self.invalidate_caches(CacheInvalidation::KEYBOARD);
    }
}
