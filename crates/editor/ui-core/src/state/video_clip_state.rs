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
    /// 缩放倍数
    pub zoom: f32,
    /// 平移偏移（像素）
    pub pan_x: f32,
    /// 平移偏移（像素）
    pub pan_y: f32,
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
            preview_width: 0.0,
            preview_height: 0.0,
        }
    }

    /// 重置视口（双击恢复）
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
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
}
