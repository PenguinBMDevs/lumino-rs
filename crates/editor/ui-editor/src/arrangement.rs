//! 工程走带视图 —— 按 yinhe 风格显示所有音轨音符的时间轴排列。
//!
//! 音符由 WGPU ArrangementRenderer 渲染。

pub mod click_canvas;
pub mod ruler;
pub mod track_list;

// 视口与交互逻辑已下沉到 iced-free 的 `lumino-arrangement-core`：
// iced 与 egui 各自把原生事件转成 `ArrangementEvent` 后调用同一份处理逻辑。
pub use lumino_arrangement_core::interaction;
pub use lumino_arrangement_core::viewport::ArrangementViewport;

/// 走带视图状态：视口 + ghost 预览 + 拖拽框选矩形。
#[derive(Debug, Clone, Default)]
pub struct ArrangementView {
    /// 视口状态
    pub viewport: ArrangementViewport,
    /// ghost 音符预览 `(tick_start, tick_end, 视觉轨, key)`，由 WGPU 叠加绘制。
    pub ghost_notes: Vec<(f64, f64, usize, u8)>,
    /// 拖拽中的框选矩形 `(tick_start, tick_end, track_lo, track_hi)`，由 WGPU 叠加绘制。
    pub drag_sel_rect: Option<(f64, f64, usize, usize)>,
}

impl ArrangementView {
    /// 新建默认视图。
    pub fn new() -> Self {
        Self::default()
    }
}

pub use click_canvas::ArrangementClickCanvas;
pub use ruler::ArrangementRulerCanvas;
pub use track_list::TrackListCanvas;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arrangement_viewport_default() {
        let vp = ArrangementViewport::default();
        assert_eq!(vp.scroll_x, 0.0);
        assert_eq!(vp.scroll_y, 0.0);
        assert_eq!(
            vp.zoom_x,
            200.0 / (lumino_core::view_state::DEFAULT_PPQ as f32 * 4.0)
        );
        assert_eq!(vp.zoom_y, 1.0);
        assert_eq!(vp.track_height, 48.0);
        assert_eq!(
            vp.canvas_size,
            lumino_message::types::geometry::Point2::new(800.0, 600.0)
        );
        assert_eq!(vp.total_ticks, lumino_core::view_state::DEFAULT_TOTAL_TICKS);
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
