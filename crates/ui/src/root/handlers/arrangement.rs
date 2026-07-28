//! 工程走带视图交互处理器
//!
//! 负责工程走带视图的滚动、缩放以及相关的走带编辑消息。

use crate::constants::editor;
use crate::root::Root;

impl Root {
    /// 处理走带视图水平滚动
    pub(crate) fn handle_arrangement_scroll_x(&mut self, x: f32) -> bool {
        // 先计算缓存的最大 tick（可能扫描 track_notes），再借用 viewport
        let max_tick = self.arrangement_max_tick_end();
        let vp = &mut self.arrangement_view.viewport;
        let canvas_w = vp.canvas_size.x.max(1.0);
        // 与 ArrangementViewport::clamp_scroll 保持一致：使用 cached_max_tick_end、
        // total_ticks 和 DEFAULT_MIN_TICKS 三者中的最大值，确保无音符时也有合理滚动范围
        let effective_max_tick = max_tick
            .max(vp.total_ticks as f32)
            .max(editor::DEFAULT_MIN_TICKS);
        let total_w = effective_max_tick * vp.zoom_x;
        let max_scroll = (total_w - canvas_w).max(0.0);
        vp.scroll_x = x.max(0.0).min(max_scroll);
        true
    }

    /// 处理走带视图垂直滚动
    pub(crate) fn handle_arrangement_scroll_y(&mut self, y: f32) -> bool {
        let vp = &mut self.arrangement_view.viewport;
        let track_count = self.sidebar.tracks.len().max(1) as f32;
        let total_h = track_count * vp.track_height * vp.zoom_y;
        let canvas_h = vp.canvas_size.y.max(1.0);
        let max_scroll = (total_h - canvas_h).max(0.0);
        vp.scroll_y = y.max(0.0).min(max_scroll);
        true
    }

    /// 处理走带视图水平缩放（固定点缩放）
    pub(crate) fn handle_arrangement_zoom_x(&mut self, zoom: f32, fixed_ratio: f32) -> bool {
        let vp = &mut self.arrangement_view.viewport;
        let old_zoom = vp.zoom_x;
        let canvas_w = vp.canvas_size.x.max(1.0);
        let min_zoom = editor::zoom::MIN_ARRANGEMENT_ZOOM_X;
        let max_zoom_x = (canvas_w / 4.0).max(min_zoom);
        let new_zoom = zoom.clamp(min_zoom, max_zoom_x);
        let focus_px = vp.scroll_x + canvas_w * fixed_ratio;
        let focus_tick = focus_px / old_zoom;
        vp.zoom_x = new_zoom;
        vp.scroll_x = (focus_tick * new_zoom - canvas_w * fixed_ratio).max(0.0);
        true
    }

    /// 处理走带视图垂直缩放（固定点缩放）
    pub(crate) fn handle_arrangement_zoom_y(&mut self, zoom: f32, fixed_ratio: f32) -> bool {
        let vp = &mut self.arrangement_view.viewport;
        let old_zoom = vp.zoom_y;
        let canvas_h = vp.canvas_size.y.max(1.0);
        let track_count = self.sidebar.tracks.len().max(1) as f32;
        let min_zoom = editor::zoom::MIN_ARRANGEMENT_ZOOM_Y;
        let max_zoom = editor::zoom::MAX_ARRANGEMENT_ZOOM_Y;
        let new_zoom = zoom.clamp(min_zoom, max_zoom);
        let focus_px = vp.scroll_y + canvas_h * fixed_ratio;
        let focus_ratio = focus_px / (old_zoom * vp.track_height);
        vp.zoom_y = new_zoom;
        let total_h = track_count * vp.track_height * new_zoom;
        let max_scroll = (total_h - canvas_h).max(0.0);
        vp.scroll_y = (focus_ratio * new_zoom * vp.track_height - canvas_h * fixed_ratio)
            .clamp(0.0, max_scroll);
        true
    }
}
