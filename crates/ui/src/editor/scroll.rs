use crate::constants::editor::zoom::{MAX_ZOOM_X, MAX_ZOOM_Y, MIN_ZOOM_X, MIN_ZOOM_Y};

impl super::Editor {
    // 滚动控制

    pub fn set_max_scroll_x(&mut self, max_scroll: f32) {
        self.max_scroll_x = max_scroll;
    }

    pub fn set_max_scroll_y(&mut self, max_scroll: f32) {
        self.max_scroll_y = max_scroll;
    }

    pub fn scroll_x(&self) -> f32 {
        self.state.scroll_x
    }

    pub fn scroll_y(&self) -> f32 {
        self.state.scroll_y
    }

    /// 获取滚动位置 (x, y)
    pub fn scroll(&self) -> (f32, f32) {
        (self.state.scroll_x, self.state.scroll_y)
    }

    /// 获取缩放 (x, y)
    pub fn zoom(&self) -> (f32, f32) {
        (self.state.zoom_x, self.state.zoom_y)
    }

    pub fn zoom_x(&self) -> f32 {
        self.state.zoom_x
    }

    pub fn zoom_y(&self) -> f32 {
        self.state.zoom_y
    }

    pub fn set_scroll_x(&mut self, scroll_x: f32) {
        // 计算实际可滚动的最大范围：总宽度 - 视口宽度
        let total_width = self.state.total_ticks as f32 * self.state.zoom_x;
        let viewport_width = (self.canvas_size.x - self.state.keyboard_width).max(0.0);
        let effective_max_scroll = (total_width - viewport_width).max(0.0);
        self.state.scroll_x = scroll_x.max(0.0).min(effective_max_scroll);
        // 水平滚动只影响标尺（和网格线，但网格线已由 wgpu 渲染）
        self.ruler_cache.clear();
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        // 计算实际可滚动的最大范围：总高度 - 视口高度（减去时间轴标尺高度）
        let total_height = self.state.visible_key_count as f32 * self.state.zoom_y;
        let viewport_height = (self.canvas_size.y - self.state.ruler_height).max(0.0);
        let effective_max_scroll = (total_height - viewport_height).max(0.0);
        self.state.scroll_y = scroll_y.max(0.0).min(effective_max_scroll);
        // 垂直滚动只影响键盘（和网格线，但网格线已由 wgpu 渲染）
        self.keyboard_cache.clear();
    }

    pub fn set_zoom_x(&mut self, zoom_x: f32, fixed_ratio: f32) {
        let old_zoom_x = self.state.zoom_x;
        self.state.zoom_x = zoom_x.clamp(MIN_ZOOM_X, MAX_ZOOM_X);

        let ratio = self.state.zoom_x / old_zoom_x;
        let view_width = (self.canvas_size.x - self.state.keyboard_width).max(0.0);

        // 保持固定比例处的 tick 不变
        let fixed_pixel = self.state.scroll_x + view_width * fixed_ratio;
        self.state.scroll_x = fixed_pixel * ratio - view_width * fixed_ratio;

        self.max_scroll_x = self.state.total_ticks as f32 * self.state.zoom_x;
        self.state.scroll_x = self.state.scroll_x.max(0.0).min(self.max_scroll_x);
        // 缩放影响标尺和网格线
        self.ruler_cache.clear();
    }

    pub fn set_zoom_y(&mut self, zoom_y: f32, fixed_ratio: f32) {
        let old_zoom_y = self.state.zoom_y;
        self.state.zoom_y = zoom_y.clamp(MIN_ZOOM_Y, MAX_ZOOM_Y);

        let ratio = self.state.zoom_y / old_zoom_y;
        let view_height = self.canvas_size.y.max(0.0);

        // 保持固定比例处的 key 不变
        let fixed_pixel = self.state.scroll_y + view_height * fixed_ratio;
        self.state.scroll_y = fixed_pixel * ratio - view_height * fixed_ratio;

        self.max_scroll_y = self.state.visible_key_count as f32 * self.state.zoom_y;
        self.state.scroll_y = self.state.scroll_y.max(0.0).min(self.max_scroll_y);
        // 缩放影响键盘和网格线
        self.keyboard_cache.clear();
    }
}
