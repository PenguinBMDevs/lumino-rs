//! 工程走带视图 —— 按 yinhe 风格显示所有音轨音符的时间轴排列。
//!
//! 音符由 WGPU ArrangementRenderer 渲染。

pub mod click_canvas;
pub mod pattern_widget;
pub mod track_list;

use iced_core::Point;
use lumino_core::Pattern;

pub use click_canvas::ArrangementClickCanvas;
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
    /// 垂直缩放（倍率，1.0 = 默认高度）
    pub zoom_y: f32,
    /// 每轨高度（像素）
    pub track_height: f32,
    /// Canvas 偏移（屏幕坐标，GPU 实例使用，每帧从 viewport_info 刷新）
    pub canvas_offset: Point,
    /// Canvas 尺寸（用于滚动条范围计算 + GPU 实例构建）
    pub canvas_size: Point,
    /// 总 tick 数
    pub total_ticks: u32,
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
            zoom_x: 0.5,
            zoom_y: 1.0,
            track_height: 48.0,
            canvas_offset: Point::new(0.0, 0.0),
            canvas_size: Point::new(800.0, 600.0),
            total_ticks: 0,
            cached_max_tick_end: 0.0,
            cached_track_notes_gen: 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arrangement_viewport_default() {
        let vp = ArrangementViewport::default();
        assert_eq!(vp.scroll_x, 0.0);
        assert_eq!(vp.scroll_y, 0.0);
        assert_eq!(vp.zoom_x, 0.5);
        assert_eq!(vp.zoom_y, 1.0);
        assert_eq!(vp.track_height, 48.0);
        assert_eq!(vp.canvas_size, Point::new(800.0, 600.0));
        assert_eq!(vp.total_ticks, 0);
        assert_eq!(vp.cached_max_tick_end, 0.0);
        assert_eq!(vp.cached_track_notes_gen, 0);
    }

    #[test]
    fn test_total_height_calculation() {
        let track_count = 5.0_f32;
        let track_height = 48.0_f32;
        let zoom_y = 1.5_f32;
        let total_height = track_count * track_height * zoom_y;
        assert_eq!(total_height, 360.0);
    }

    #[test]
    fn test_max_scroll_vertical() {
        let track_count = 10.0_f32;
        let track_height = 48.0_f32;
        let zoom_y = 1.0_f32;
        let canvas_height = 600.0_f32;
        let total_height = track_count * track_height * zoom_y;
        let max_scroll = (total_height - canvas_height).max(0.0);
        assert_eq!(max_scroll, 0.0); // 10 * 48 = 480 < 600
    }

    #[test]
    fn test_max_scroll_vertical_with_zoom() {
        let track_count = 10.0_f32;
        let track_height = 48.0_f32;
        let zoom_y = 2.0_f32;
        let canvas_height = 600.0_f32;
        let total_height = track_count * track_height * zoom_y;
        let max_scroll = (total_height - canvas_height).max(0.0);
        assert_eq!(max_scroll, 360.0); // 10 * 48 * 2 = 960, 960 - 600 = 360
    }

    #[test]
    fn test_max_zoom_for_tracks() {
        let canvas_height = 600.0_f32;
        let track_count = 10.0_f32;
        let track_height = 48.0_f32;
        let max_zoom = (canvas_height / (track_count * track_height)).max(0.2);
        assert!((max_zoom - 1.25).abs() < 0.01); // 600 / (10 * 48) = 1.25
    }

    #[test]
    fn test_max_zoom_minimum() {
        let canvas_height = 100.0_f32;
        let track_count = 100.0_f32;
        let track_height = 48.0_f32;
        let max_zoom = (canvas_height / (track_count * track_height)).max(0.2);
        assert_eq!(max_zoom, 0.2); // 100 / 4800 = 0.02, min is 0.2
    }

    #[test]
    fn test_scroll_y_clamping() {
        let track_count = 5.0_f32;
        let track_height = 48.0_f32;
        let zoom_y = 2.0_f32;
        let canvas_height = 600.0_f32;
        let total_height = track_count * track_height * zoom_y;
        let max_scroll = (total_height - canvas_height).max(0.0);

        // total_height = 5 * 48 * 2 = 480 < 600, so max_scroll = 0
        assert_eq!(max_scroll, 0.0);

        // scroll_y should be clamped to 0
        let scroll_y = 100.0_f32;
        let clamped = scroll_y.max(0.0).min(max_scroll);
        assert_eq!(clamped, 0.0);
    }

    #[test]
    fn test_scroll_y_with_larger_content() {
        let track_count = 20.0_f32;
        let track_height = 48.0_f32;
        let zoom_y = 1.0_f32;
        let canvas_height = 600.0_f32;
        let total_height = track_count * track_height * zoom_y;
        let max_scroll = (total_height - canvas_height).max(0.0);

        // total_height = 20 * 48 = 960, max_scroll = 360
        assert_eq!(max_scroll, 360.0);

        // scroll_y should be clamped to 360
        let scroll_y = 500.0_f32;
        let clamped = scroll_y.max(0.0).min(max_scroll);
        assert_eq!(clamped, 360.0);
    }
}
