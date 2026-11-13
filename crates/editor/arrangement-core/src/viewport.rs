//! 工程走带视口（框架无关）。
//!
//! 从 `lumino-ui-editor/src/arrangement.rs` 抽出：原实现只用 旧版 `Point` 表示
//! canvas 偏移/尺寸，替换成中立的 `Point2` 后即可被 旧版 与 egui 共用。

use lumino_message::types::geometry::Point2;

/// 走带视图 X 轴最小缩放。
pub const MIN_ARRANGEMENT_ZOOM_X: f32 = 0.01;
/// 走带视图 X 轴最大缩放。
pub const MAX_ARRANGEMENT_ZOOM_X: f32 = 10.0;
/// 默认最小 tick 值（用于画布宽度下限）。
pub const DEFAULT_MIN_TICKS: f32 = 960.0 * 4.0;

/// 工程走带视口状态
#[derive(Debug, Clone)]
pub struct ArrangementViewport {
    /// 水平滚动（像素）
    pub scroll_x: f32,
    /// 垂直滚动（像素）
    pub scroll_y: f32,
    /// 水平缩放（像素/tick）
    pub zoom_x: f32,
    /// 垂直缩放（倍率，1.0 = 默认高度）
    pub zoom_y: f32,
    /// 每轨高度（像素）
    pub track_height: f32,
    /// Canvas 偏移（屏幕坐标，GPU 实例使用，每帧从 viewport_info 刷新）
    pub canvas_offset: Point2,
    /// Canvas 尺寸（用于滚动条范围计算 + GPU 实例构建）
    pub canvas_size: Point2,
    /// 总 tick 数
    pub total_ticks: u32,
    /// 分辨率 (Pulses Per Quarter note)
    pub ppq: u16,
    /// 缓存的音符最大 tick 终点，避免每帧全量扫描 track_notes
    pub cached_max_tick_end: f32,
    /// 缓存失效版本号，对应 EditorData::track_notes_gen
    pub cached_track_notes_gen: u64,
}

impl Default for ArrangementViewport {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            // 200px 一个小节：tpb = DEFAULT_PPQ * 4 = 7680 ticks/bar, 7680 * zoom_x = 200
            zoom_x: 200.0 / (lumino_core::view_state::DEFAULT_PPQ as f32 * 4.0),
            zoom_y: 1.0,
            track_height: 48.0,
            canvas_offset: Point2::new(0.0, 0.0),
            canvas_size: Point2::new(800.0, 600.0),
            total_ticks: lumino_core::view_state::DEFAULT_TOTAL_TICKS,
            ppq: lumino_core::view_state::DEFAULT_PPQ,
            cached_max_tick_end: 0.0,
            cached_track_notes_gen: 0,
        }
    }
}

impl ArrangementViewport {
    /// 当前每轨显示高度（像素）
    #[inline]
    pub fn lane_height(&self) -> f32 {
        self.track_height * self.zoom_y
    }

    /// tick 转换为视图局部 x 坐标（不含滚动偏移）
    #[inline]
    pub fn tick_to_x(&self, tick: f64) -> f32 {
        tick as f32 * self.zoom_x
    }

    /// 视图局部 x 坐标转换为 tick
    #[inline]
    pub fn x_to_tick(&self, x: f32) -> f64 {
        x as f64 / self.zoom_x as f64
    }

    /// 指定音轨的视图局部 y 坐标（不含滚动偏移）
    #[inline]
    pub fn lane_y(&self, track_idx: usize) -> f32 {
        track_idx as f32 * self.lane_height()
    }

    /// 当前可见的音轨范围
    pub fn visible_track_range(&self, num_tracks: usize) -> (usize, usize) {
        let lane_height = self.lane_height();
        let first =
            ((self.scroll_y / lane_height).floor() as usize).min(num_tracks.saturating_sub(1));
        let visible_count = (self.canvas_size.y / lane_height).ceil() as usize + 1;
        let last = (first + visible_count).min(num_tracks);
        (first, last)
    }

    /// 限制滚动范围，避免视图超出内容边界
    pub fn clamp_scroll(&mut self, num_tracks: usize) {
        let lane_height = self.lane_height();
        let max_scroll_y = (num_tracks as f32 * lane_height - self.canvas_size.y).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y);

        let effective_max_tick = self
            .cached_max_tick_end
            .max(self.total_ticks as f32)
            .max(crate::viewport::DEFAULT_MIN_TICKS);
        let max_scroll_x = (effective_max_tick * self.zoom_x - self.canvas_size.x).max(0.0);
        self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x);
    }

    /// 以指针位置为中心水平缩放
    pub fn zoom_around_x(&mut self, pointer_x: f32, factor: f32) {
        let tick_at_pointer = (self.scroll_x + pointer_x) as f64 / self.zoom_x as f64;
        self.zoom_x = (self.zoom_x * factor).clamp(MIN_ARRANGEMENT_ZOOM_X, MAX_ARRANGEMENT_ZOOM_X);
        self.scroll_x = (tick_at_pointer * self.zoom_x as f64 - pointer_x as f64) as f32;
        self.scroll_x = self.scroll_x.max(0.0);
    }

    /// 以指针位置为中心垂直缩放
    pub fn zoom_lane_height(&mut self, pointer_y: f32, factor: f32) {
        let old_height = self.lane_height();
        self.zoom_y = (self.zoom_y * factor).clamp(0.2, 5.0);
        let new_height = self.lane_height();
        let track_frac = (self.scroll_y + pointer_y) / old_height;
        self.scroll_y = track_frac * new_height - pointer_y;
        self.scroll_y = self.scroll_y.max(0.0);
    }
}

/// 工程走带视图（纯状态容器）
#[derive(Debug, Clone, Default)]
pub struct ArrangementView {
    /// 视口状态
    pub viewport: ArrangementViewport,
    /// 移动拖拽时 ghost 音符预览 `(tick_start, tick_end, 视觉轨, key)`，由 WGPU 渲染。
    ///
    /// `key` 供覆盖层按真实音符公式定位纵向位置；`视觉轨` 为侧边栏顺序索引
    /// （取色时由覆盖层经 `track_order` 映射回文档轨）。
    pub ghost_notes: Vec<(f64, f64, usize, u8)>,
    /// 拖拽中的框选矩形（tick_start, tick_end, track_lo, track_hi），由 WGPU 渲染。
    /// 覆盖 Pointer 框选、移动拖拽、Eraser 拖拽三种场景。
    pub drag_sel_rect: Option<(f64, f64, usize, usize)>,
}

impl ArrangementView {
    /// 创建默认的工程走带视图。
    ///
    /// # 返回
    /// 返回使用默认配置初始化的 `ArrangementView`。
    pub fn new() -> Self {
        Self::default()
    }
}
