//! 视频剪辑面板状态（瀑布流预览）
//!
//! 模仿 nezha `piano_view::show` 的 zoom / pan 交互模型，
//! 为 Lumino 视频剪辑窗口提供独立的预览视口状态。

/// 最小缩放倍数（对标 nezha MIN_ZOOM）
pub const VIDEO_CLIP_MIN_ZOOM: f32 = 1.0;
/// 最大缩放倍数（对标 nezha MAX_ZOOM）
pub const VIDEO_CLIP_MAX_ZOOM: f32 = 10.0;
/// 滚轮缩放系数（对标 nezha ZOOM_SCROLL_FACTOR）
pub const VIDEO_CLIP_ZOOM_SCROLL_FACTOR: f32 = 1.1;

/// 视频剪辑预览状态
///
/// 仅保存视口交互状态（zoom / pan），不持有纹理。
/// 渲染所需离屏纹理通过 `Host.waterfall_player.view` 共享。
#[derive(Debug, Clone)]
pub struct VideoClipState {
    /// 缩放倍数（剪辑带时间轴与 header 显示共用，保证 UI 一致）
    pub zoom: f32,
    /// 平移偏移（像素）
    pub pan_x: f32,
    /// 平移偏移（像素）
    pub pan_y: f32,
    /// 时间轴水平滚动位置（像素，来自滚动条/滚轮）
    pub timeline_scroll_x: f32,
    /// 预览区内容宽度（由 responsive 回调写入）
    pub preview_width: f32,
    /// 预览区内容高度
    pub preview_height: f32,
}

impl Default for VideoClipState {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoClipState {
    /// 创建默认状态
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            timeline_scroll_x: 0.0,
            preview_width: 0.0,
            preview_height: 0.0,
        }
    }

    /// 重置视口（双击恢复）
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.timeline_scroll_x = 0.0;
    }

    /// 设置时间轴滚动位置并钳制到有效范围
    ///
    /// `content_w` 为缩放后内容总宽，`viewport_w` 为时间轴可视宽度；
    /// 有效范围 `[0, (content_w - viewport_w).max(0)]`。
    pub fn set_timeline_scroll(&mut self, x: f32, content_w: f32, viewport_w: f32) {
        let max_scroll = (content_w - viewport_w).max(0.0);
        self.timeline_scroll_x = x.clamp(0.0, max_scroll);
    }

    /// 时间轴锚点缩放：以 `fixed_ratio`（视口内横向比例）为锚点调整缩放，
    /// 保证锚点处的内容点在缩放前后屏幕位置不动，随后钳制滚动。
    pub fn timeline_zoom_around(
        &mut self,
        new_zoom: f32,
        fixed_ratio: f32,
        old_zoom: f32,
        content_base_w: f32,
        viewport_w: f32,
    ) {
        let new_zoom = new_zoom.clamp(VIDEO_CLIP_MIN_ZOOM, VIDEO_CLIP_MAX_ZOOM);
        if old_zoom <= 0.0 || new_zoom <= 0.0 {
            return;
        }
        let anchor_px = fixed_ratio * viewport_w;
        let content_x = self.timeline_scroll_x + anchor_px;
        let scale = new_zoom / old_zoom;
        let new_scroll = content_x * scale - anchor_px;
        self.zoom = new_zoom;
        let content_w = content_base_w * new_zoom;
        self.set_timeline_scroll(new_scroll, content_w, viewport_w);
    }

    /// 应用缩放（限制在 [MIN, MAX]）
    pub fn apply_zoom(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(VIDEO_CLIP_MIN_ZOOM, VIDEO_CLIP_MAX_ZOOM);
    }

    /// 设置缩放（直接赋值并 clamp）
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(VIDEO_CLIP_MIN_ZOOM, VIDEO_CLIP_MAX_ZOOM);
    }

    /// 平移
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    /// 以锚点为中心缩放（模拟 nezha 鼠标锚点逻辑）
    #[allow(clippy::too_many_arguments)]
    pub fn zoom_around(
        &mut self,
        old_zoom: f32,
        new_zoom: f32,
        anchor_x: f32,
        anchor_y: f32,
        viewport_center_x: f32,
        viewport_center_y: f32,
        cursor_x: f32,
        cursor_y: f32,
    ) {
        let center_x = viewport_center_x;
        let center_y = viewport_center_y;
        let anchor_view_x = (cursor_x - center_x - self.pan_x) / old_zoom;
        let anchor_view_y = (cursor_y - center_y - self.pan_y) / old_zoom;
        let _ = (anchor_x, anchor_y);
        self.pan_x = cursor_x - center_x - new_zoom * anchor_view_x;
        self.pan_y = cursor_y - center_y - new_zoom * anchor_view_y;
    }

    /// 限制 pan 在最大可平移范围内
    pub fn clamp_pan(&mut self, available_w: f32, available_h: f32, base_w: f32, base_h: f32) {
        let scaled_w = base_w * self.zoom;
        let scaled_h = base_h * self.zoom;
        let excess_x = (scaled_w - available_w).max(0.0) / 2.0;
        let excess_y = (scaled_h - available_h).max(0.0) / 2.0;
        self.pan_x = self.pan_x.clamp(-excess_x, excess_x);
        self.pan_y = self.pan_y.clamp(-excess_y, excess_y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let s = VideoClipState::new();
        assert!((s.zoom - 1.0).abs() < f32::EPSILON);
        assert!((s.pan_x).abs() < f32::EPSILON);
    }

    #[test]
    fn test_zoom_clamp() {
        let mut s = VideoClipState::new();
        s.apply_zoom(100.0);
        assert!(s.zoom <= VIDEO_CLIP_MAX_ZOOM);
        s.set_zoom(0.1);
        assert!(s.zoom >= VIDEO_CLIP_MIN_ZOOM);
    }

    #[test]
    fn test_reset() {
        let mut s = VideoClipState::new();
        s.zoom = 5.0;
        s.pan_x = 100.0;
        s.reset_view();
        assert!((s.zoom - 1.0).abs() < f32::EPSILON);
        assert!((s.pan_x).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pan_clamp() {
        let mut s = VideoClipState::new();
        s.zoom = 2.0;
        s.pan_x = 1000.0;
        s.clamp_pan(800.0, 600.0, 400.0, 300.0);
        // scaled 800x600 vs available 800x600 -> excess 0 -> pan 0
        assert!((s.pan_x).abs() < f32::EPSILON);
    }

    #[test]
    fn test_timeline_scroll_clamp() {
        let mut s = VideoClipState::new();
        // 内容 2000 宽、视口 800 → 有效范围 [0, 1200]
        s.set_timeline_scroll(5000.0, 2000.0, 800.0);
        assert!((s.timeline_scroll_x - 1200.0).abs() < f32::EPSILON);
        s.set_timeline_scroll(-100.0, 2000.0, 800.0);
        assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
        // 内容不足视口 → 恒为 0
        s.set_timeline_scroll(50.0, 400.0, 800.0);
        assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
    }

    #[test]
    fn test_timeline_zoom_around_anchor_stays_put() {
        let mut s = VideoClipState::new();
        let base_w = 1600.0; // 未缩放内容宽
        let viewport = 800.0;
        // 先滚到 200，锚点在视口右端（ratio=1.0）
        s.set_timeline_scroll(200.0, base_w, viewport);
        let old_zoom = s.zoom;
        s.timeline_zoom_around(2.0, 1.0, old_zoom, base_w, viewport);
        // 锚点内容坐标：200+800=1000；放大 2 倍后该点仍在视口右端：
        // new_scroll = 1000*2 - 800 = 1200
        assert!((s.timeline_scroll_x - 1200.0).abs() < 0.01);
        assert!((s.zoom - 2.0).abs() < f32::EPSILON);

        // 缩小回 1.0（锚点左端 ratio=0）：左缘内容点 1200 等比映射 → scroll = 1200*0.5 = 600，
        // 且该内容点仍位于视口左缘（锚点不动性的本质）
        s.timeline_zoom_around(1.0, 0.0, s.zoom, base_w, viewport);
        assert!((s.timeline_scroll_x - 600.0).abs() < 0.01);
        assert!((s.zoom - 1.0).abs() < f32::EPSILON);

        // 钳制：继续放大 10 倍时 scroll 不越界
        s.timeline_zoom_around(10.0, 1.0, s.zoom, base_w, viewport);
        let max_scroll = (base_w * 10.0 - viewport).max(0.0);
        assert!(s.timeline_scroll_x <= max_scroll + 0.01);
    }

    #[test]
    fn test_reset_clears_timeline_scroll() {
        let mut s = VideoClipState::new();
        s.timeline_scroll_x = 300.0;
        s.reset_view();
        assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
    }
}
