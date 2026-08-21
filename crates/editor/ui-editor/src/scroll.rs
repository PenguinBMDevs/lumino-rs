use super::CacheInvalidation;
use lumino_editor_state::editor_state::viewport::Viewport;
use lumino_ui_core::constants::editor::zoom::{MAX_ZOOM_X, MAX_ZOOM_Y, MIN_ZOOM_X, MIN_ZOOM_Y};

impl super::Editor {
    // 滚动控制 — 直接通过 viewport 模块操作

    /// 设置水平最大滚动范围。
    ///
    /// # 参数
    /// * `max_scroll` — 水平最大滚动值
    pub fn set_max_scroll_x(&mut self, max_scroll: f32) {
        self.editor_state.max_scroll.0 = max_scroll;
    }

    /// 设置垂直最大滚动范围。
    ///
    /// # 参数
    /// * `max_scroll` — 垂直最大滚动值
    pub fn set_max_scroll_y(&mut self, max_scroll: f32) {
        self.editor_state.max_scroll.1 = max_scroll;
    }

    /// 获取当前水平滚动位置。
    ///
    /// # 返回
    /// 水平滚动值。
    pub fn scroll_x(&self) -> f32 {
        self.editor_state.view.scroll_x
    }

    /// 获取当前垂直滚动位置。
    ///
    /// # 返回
    /// 垂直滚动值。
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

    /// 获取当前水平缩放倍率。
    ///
    /// # 返回
    /// 水平缩放值。
    pub fn zoom_x(&self) -> f32 {
        self.editor_state.view.zoom_x
    }

    /// 获取当前垂直缩放倍率。
    ///
    /// # 返回
    /// 垂直缩放值。
    pub fn zoom_y(&self) -> f32 {
        self.editor_state.view.zoom_y
    }

    /// 设置水平滚动位置（钳位到有效范围并刷新标尺缓存）。
    ///
    /// 纵向卷帘复用本方法驱动时间轴偏移：`keyboard_width` 传键盘高度、
    /// `canvas_width` 传画布高度、`time_zoom` 传 `zoom_y`。
    ///
    /// # 参数
    /// * `scroll_x` — 目标水平滚动值（纵向模式下即时间轴偏移）
    /// * `keyboard_width` — pitch 轴方向留白尺寸
    /// * `canvas_width` — 视口尺寸（与 `keyboard_width` 同向）
    /// * `time_zoom` — 时间轴像素缩放
    pub fn set_scroll_x(
        &mut self,
        scroll_x: f32,
        keyboard_width: f32,
        canvas_width: f32,
        time_zoom: f32,
    ) {
        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_scroll_x(scroll_x, keyboard_width, canvas_width, time_zoom);
        self.invalidate_caches(CacheInvalidation::RULER);
    }

    /// 设置垂直滚动位置（钳位到有效范围并刷新键盘缓存）。
    ///
    /// # 参数
    /// * `scroll_y` — 目标垂直滚动值
    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        let canvas_height = self.editor_state.canvas.size_y;
        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_scroll_y(scroll_y, canvas_height);
        self.invalidate_caches(CacheInvalidation::KEYBOARD);
    }

    /// 设置水平缩放倍率（以固定比例锚定缩放并刷新标尺缓存）。
    ///
    /// # 参数
    /// * `zoom_x` — 目标水平缩放倍率
    /// * `fixed_ratio` — 缩放锚点在视口中的固定比例
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

    /// 设置垂直缩放倍率（以固定比例锚定缩放并刷新键盘缓存）。
    ///
    /// 根据可视键数动态限制最小/最大缩放，防止键盘过度缩放出界。
    ///
    /// # 参数
    /// * `zoom_y` — 目标垂直缩放倍率
    /// * `fixed_ratio` — 缩放锚点在视口中的固定比例
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

    /// 设置纵向卷帘「音高轴」滚动位置（键盘 X 向平移）。
    ///
    /// 使用音高轴专用坐标（`pitch_zoom`/`key_count`/`max_scroll.1`）钳制，**不碰**
    /// 横向键盘的 `scroll_y`/`zoom_y`，与 `ScrollbarScrolledY` 配套。
    pub fn set_pitch_scroll(&mut self, _pitch_scroll: f32) {
        // 纵向卷帘面板已改为直接复用瀑布流播放器（键盘 + 卷帘由 GPU 离屏渲染），
        // 不再维护独立的 `pitch_zoom`/`pitch_scroll` 字段；该分支暂为占位，
        // 后续挂载音符编辑动作时再实现对应缩放/平移。保持函数签名以保留调用点稳定。
    }

    /// 设置纵向键盘水平滚动（仅纵向卷帘，视口为画布宽度）
    pub fn set_vertical_keyboard_scroll(&mut self, scroll_y: f32) {
        let canvas_width = self.editor_state.canvas.size_x;
        let total_width =
            self.editor_state.view.visible_key_count as f32 * self.editor_state.view.zoom_y;
        let max_scroll = (total_width - canvas_width).max(0.0);
        self.editor_state.view.scroll_y = scroll_y.clamp(0.0, max_scroll);
        self.editor_state.view.smooth_scroll.target_y = self.editor_state.view.scroll_y;
        self.editor_state.view.smooth_scroll.active = false;
        self.invalidate_caches(CacheInvalidation::KEYBOARD);
    }

    /// 设置纵向键盘缩放（仅纵向卷帘，视口为画布宽度，锚点为水平比例）
    pub fn set_vertical_keyboard_zoom(&mut self, zoom_y: f32, fixed_ratio: f32) {
        let canvas_width = self.editor_state.canvas.size_x;
        let visible_key_count = self.editor_state.view.visible_key_count;
        let old = self.editor_state.view.zoom_y;

        let dynamic_min_zoom = if visible_key_count > 0 && canvas_width > 0.0 {
            (canvas_width / visible_key_count as f32).clamp(MIN_ZOOM_Y, MAX_ZOOM_Y)
        } else {
            MIN_ZOOM_Y
        };

        const MAX_SCROLLABLE_PAGES: f32 = 16.0;
        let dynamic_max_zoom = if visible_key_count > 0 && canvas_width > 0.0 {
            MAX_ZOOM_Y
                .min(canvas_width * MAX_SCROLLABLE_PAGES / visible_key_count as f32)
                .max(MIN_ZOOM_Y)
        } else {
            MAX_ZOOM_Y
        };

        let new_zoom = zoom_y.clamp(dynamic_min_zoom, dynamic_max_zoom);
        if (new_zoom - old).abs() < f32::EPSILON {
            return;
        }
        let ratio = new_zoom / old.max(f32::EPSILON);
        let viewport = canvas_width.max(0.0);
        let fixed_point = self.editor_state.view.scroll_y + viewport * fixed_ratio;
        self.editor_state.view.scroll_y = fixed_point * ratio - viewport * fixed_ratio;
        self.editor_state.view.zoom_y = new_zoom;
        self.editor_state.max_scroll.1 = visible_key_count as f32 * new_zoom;
        let max_scroll = (self.editor_state.max_scroll.1 - viewport).max(0.0);
        self.editor_state.view.scroll_y = self.editor_state.view.scroll_y.clamp(0.0, max_scroll);
        self.editor_state.view.smooth_scroll.target_y = self.editor_state.view.scroll_y;
        self.editor_state.view.smooth_scroll.active = false;
        self.invalidate_caches(CacheInvalidation::KEYBOARD);
    }

    /// 使纵向键盘完整显示 128/256 键（重置缩放与滚动以铺满视口宽度）
    pub fn fit_vertical_keyboard_to_viewport(&mut self) {
        let canvas_width = self.editor_state.canvas.size_x;
        if canvas_width <= 1.0 {
            return;
        }
        let visible = self.editor_state.view.visible_key_count as f32;
        if visible <= 0.0 {
            return;
        }
        let target_zoom = (canvas_width / visible).clamp(MIN_ZOOM_Y, MAX_ZOOM_Y);
        self.editor_state.view.zoom_y = target_zoom;
        self.editor_state.max_scroll.1 = visible * target_zoom;
        self.editor_state.view.scroll_y = 0.0;
        self.editor_state.view.smooth_scroll.target_y = 0.0;
        self.editor_state.view.smooth_scroll.active = false;
        self.invalidate_caches(CacheInvalidation::KEYBOARD);
    }

    /// 设置纵向时间轴滚动（Y 向，头部对齐键盘顶部，向远离键盘方向递增）
    pub fn set_vertical_time_scroll(&mut self, scroll_x: f32) {
        let canvas_height = self.editor_state.canvas.size_y;
        let keyboard_h = self.editor_state.view.keyboard_width;
        let ruler_h = self.editor_state.view.ruler_height;
        let grid_h = (canvas_height - ruler_h - keyboard_h).max(0.0);
        let total_h = self.editor_state.view.total_ticks as f32 * self.editor_state.view.zoom_x;
        let max_scroll = (total_h - grid_h).max(0.0);
        self.editor_state.view.scroll_x = scroll_x.clamp(0.0, max_scroll);
        self.editor_state.view.smooth_scroll.target_x = self.editor_state.view.scroll_x;
        self.editor_state.view.smooth_scroll.active = false;
        self.invalidate_caches(CacheInvalidation::RULER);
    }

    /// 设置纵向时间轴缩放（Y 向，锚点为距键盘顶部比例，0=键盘顶部，1=顶部标尺）
    pub fn set_vertical_time_zoom(&mut self, zoom_x: f32, fixed_ratio: f32) {
        let canvas_height = self.editor_state.canvas.size_y;
        let keyboard_h = self.editor_state.view.keyboard_width;
        let ruler_h = self.editor_state.view.ruler_height;
        let grid_h = (canvas_height - ruler_h - keyboard_h).max(0.0);
        let old = self.editor_state.view.zoom_x;
        let new_zoom = zoom_x.clamp(MIN_ZOOM_X, MAX_ZOOM_X);
        if (new_zoom - old).abs() < f32::EPSILON || grid_h <= 0.0 {
            return;
        }
        let ratio = new_zoom / old.max(f32::EPSILON);
        // fixed_ratio 0 在键盘顶部（grid_bottom），1 在顶部标尺（grid_top），即距底部距离 = grid_h * fixed_ratio
        // 推导：tick = (grid_bottom - pointer_y + scroll)/zoom = (dist_from_bottom + scroll)/zoom
        // 保持 tick 不变：(dist + new_scroll)/new_zoom = (dist + old_scroll)/old_zoom
        let dist_from_bottom = grid_h * fixed_ratio.clamp(0.0, 1.0);
        let fixed_point = self.editor_state.view.scroll_x + dist_from_bottom;
        let new_scroll = fixed_point * ratio - dist_from_bottom;
        self.editor_state.view.zoom_x = new_zoom;
        self.editor_state.view.scroll_x = new_scroll;
        self.editor_state.max_scroll.0 = self.editor_state.view.total_ticks as f32 * new_zoom;
        let max_scroll = (self.editor_state.max_scroll.0 - grid_h).max(0.0);
        self.editor_state.view.scroll_x = self.editor_state.view.scroll_x.clamp(0.0, max_scroll);
        self.editor_state.view.smooth_scroll.target_x = self.editor_state.view.scroll_x;
        self.editor_state.view.smooth_scroll.active = false;
        self.invalidate_caches(CacheInvalidation::RULER);
    }
}
