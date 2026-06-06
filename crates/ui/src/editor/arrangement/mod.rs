//! 工程走带视图 —— 按 yinhe 风格显示所有音轨音符的时间轴排列。
//!
//! 音符由 WGPU ArrangementRenderer 渲染。

pub mod pattern_widget;
pub mod track_list;

use iced_core::Point;
use lumino_core::Pattern;

pub use pattern_widget::{PatternWidget, PatternWidgetState};
pub use track_list::TrackListCanvas;

/// 工程走带视口状态
#[derive(Debug, Clone)]
pub struct ArrangementViewport {
    /// 水平滚动（像素）
    pub scroll_x: f32,
    /// 垂直滚动（像素）
    pub scroll_y: f32,
    /// 水平缩放（像素/tick）
    pub zoom_x: f32,
    /// 每轨高度（像素）
    pub track_height: f32,
    /// Canvas 偏移（屏幕坐标，GPU 实例使用，每帧从 viewport_info 刷新）
    pub canvas_offset: Point,
    /// Canvas 尺寸（用于滚动条范围计算 + GPU 实例构建）
    pub canvas_size: Point,
    /// 总 tick 数
    pub total_ticks: u32,
}

impl Default for ArrangementViewport {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 0.5,
            track_height: 48.0,
            canvas_offset: Point::new(0.0, 0.0),
            canvas_size: Point::new(800.0, 600.0),
            total_ticks: 0,
        }
    }
}

/// 工程走带视图（纯状态容器）
#[derive(Debug, Clone, Default)]
pub struct ArrangementView {
    /// 视口状态
    pub viewport: ArrangementViewport,
    /// Pattern 列表（音轨总览中的音符片段）
    pub patterns: Vec<Pattern>,
}

impl ArrangementView {
    pub fn new() -> Self {
        Self::default()
    }
}
