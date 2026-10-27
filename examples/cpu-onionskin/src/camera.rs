//! 相机 / 视口：与主程序 `ViewState` 的坐标口径一致（scroll 为像素）。
//!
//! 横向：`x = tick * zoom_x - scroll_x + keyboard_width`
//! 纵向：`y = (max_key - key) * zoom_y - scroll_y + ruler_height`
//!
//! 与主程序 UI 的差异只在默认布局：示例使用固定的 128 键纵轴并允许垂直滚动，
//! 默认 `zoom_y = 20`（与 `DEFAULT_ZOOM_Y` 一致），因此音符描边/填充比例与
//! 主程序一致（UI 侧 border_width 固定 1 → 着色器实际边框 2px）。

use std::f32::consts::E;

/// 与 `lumino_core::view_state` 一致的默认值。
pub const DEFAULT_KEYBOARD_WIDTH: f32 = 120.0;
pub const DEFAULT_RULER_HEIGHT: f32 = 24.0;
pub const DEFAULT_ZOOM_Y: f32 = 20.0;
pub const KEY_COUNT: u16 = 128;

/// 相机与视口。
#[derive(Debug, Clone)]
pub struct Camera {
    /// 当前横向滚动（像素）。
    pub scroll_x: f32,
    /// 目标横向滚动（像素）——滚轮/动画终点。
    pub target_scroll_x: f32,
    /// 当前纵向滚动（像素，0 = 最顶部的 key 127）。
    pub scroll_y: f32,
    /// 横向缩放（像素 / tick）。
    pub zoom_x: f32,
    /// 纵向缩放（像素 / key）。
    pub zoom_y: f32,
    /// 键盘栏宽度（像素）。
    pub keyboard_width: f32,
    /// 标尺高度（像素）。
    pub ruler_height: f32,
    /// 纵轴键数。
    pub key_count: u16,
    /// MIDI PPQ（用于小节/拍网格）。
    pub ppq: u16,
    /// 时间轴总长（tick，含尾部空白）。
    pub total_ticks: u32,
    /// 窗口物理尺寸。
    pub width: u32,
    pub height: u32,
    /// DPI 缩放因子。
    pub scale_factor: f32,
}

impl Camera {
    /// 缩放到低分辨率离屏视口（总览热力图用）。
    #[must_use]
    pub fn scaled(&self, factor: f32) -> Self {
        let factor = factor.clamp(0.1, 1.0);
        let mut camera = self.clone();
        camera.width = ((self.width as f32 * factor).round() as u32).max(1);
        camera.height = ((self.height as f32 * factor).round() as u32).max(1);
        camera.keyboard_width *= factor;
        camera.ruler_height *= factor;
        camera.zoom_x = (self.zoom_x * factor).max(1e-6);
        camera.zoom_y = (self.zoom_y * factor).max(1e-6);
        camera.scroll_x *= factor;
        camera.target_scroll_x *= factor;
        camera.scroll_y *= factor;
        camera
    }

    /// 创建相机；默认纵向居中到中央 C 附近。
    #[must_use]
    pub fn new(
        width: u32,
        height: u32,
        scale_factor: f32,
        total_ticks: u32,
        ppq: u16,
        zoom_x: f32,
    ) -> Self {
        let keyboard_width = DEFAULT_KEYBOARD_WIDTH * scale_factor;
        let ruler_height = DEFAULT_RULER_HEIGHT * scale_factor;
        let zoom_y = DEFAULT_ZOOM_Y * scale_factor;
        let mut camera = Self {
            scroll_x: 0.0,
            target_scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x,
            zoom_y,
            keyboard_width,
            ruler_height,
            key_count: KEY_COUNT,
            ppq: ppq.max(1),
            total_ticks,
            width,
            height,
            scale_factor,
        };
        camera.center_on_key(60);
        camera
    }

    /// 画布区域尺寸。
    #[inline]
    #[must_use]
    pub fn canvas_width(&self) -> f32 {
        (self.width as f32 - self.keyboard_width).max(1.0)
    }

    #[inline]
    #[must_use]
    pub fn canvas_height(&self) -> f32 {
        (self.height as f32 - self.ruler_height).max(1.0)
    }

    /// 最大横向滚动。
    #[inline]
    #[must_use]
    pub fn max_scroll_x(&self) -> f32 {
        (self.total_ticks as f32 * self.zoom_x - self.canvas_width()).max(0.0)
    }

    /// 最大纵向滚动。
    #[inline]
    #[must_use]
    pub fn max_scroll_y(&self) -> f32 {
        (self.key_count as f32 * self.zoom_y - self.canvas_height()).max(0.0)
    }

    /// tick → 视口 x。
    #[inline]
    #[must_use]
    pub fn tick_to_x(&self, tick: f32) -> f32 {
        tick * self.zoom_x - self.scroll_x + self.keyboard_width
    }

    /// 视口 x → tick。
    #[inline]
    #[must_use]
    pub fn x_to_tick(&self, x: f32) -> f32 {
        (x - self.keyboard_width + self.scroll_x) / self.zoom_x
    }

    /// key → 视口 y。
    #[inline]
    #[must_use]
    pub fn key_to_y(&self, key: u16) -> f32 {
        (self.key_count - 1 - key) as f32 * self.zoom_y - self.scroll_y + self.ruler_height
    }

    /// 视口 y → key（浮点，未取整）。
    #[inline]
    #[must_use]
    pub fn y_to_key_f32(&self, y: f32) -> f32 {
        let max_key = (self.key_count - 1) as f32;
        max_key - (y - self.ruler_height + self.scroll_y) / self.zoom_y
    }

    /// 视口 y → key（取整并夹紧）。
    #[must_use]
    pub fn y_to_key(&self, y: f32) -> u16 {
        self.y_to_key_f32(y)
            .round()
            .clamp(0.0, (self.key_count - 1) as f32) as u16
    }

    /// 可见 key 区间（含端点）。
    #[must_use]
    pub fn visible_key_range(&self) -> (u16, u16) {
        let top = self.y_to_key_f32(self.ruler_height).ceil();
        let bottom = self.y_to_key_f32(self.height as f32).floor();
        let max_key = (self.key_count - 1) as f32;
        let hi = top.clamp(0.0, max_key) as u16;
        let lo = bottom.clamp(0.0, max_key) as u16;
        (lo.min(hi), hi.max(lo))
    }

    /// 可见 tick 区间（含端点，自动夹到 [0, total_ticks]）。
    #[must_use]
    pub fn visible_tick_range(&self) -> (u32, u32) {
        self.tick_range_for_x(self.keyboard_width, self.width as f32)
    }

    /// 指定屏幕 x 区间的 tick 区间（含端点，自动夹到 [0, total_ticks]）。
    ///
    /// band 渲染必须用**本 band 的 x 区间**，否则每个 band 都会扫全视口。
    #[must_use]
    pub fn tick_range_for_x(&self, x0: f32, x1: f32) -> (u32, u32) {
        let start = self.x_to_tick(x0).floor().max(0.0);
        let end = self
            .x_to_tick(x1)
            .ceil()
            .max(start + 1.0)
            .min(self.total_ticks as f32);
        (start as u32, end as u32)
    }

    /// 滚轮横向滚动（像素）。
    pub fn scroll_x_by(&mut self, delta_px: f32) {
        self.target_scroll_x = (self.target_scroll_x + delta_px).clamp(0.0, self.max_scroll_x());
    }

    /// 滚轮纵向滚动（像素）。
    pub fn scroll_y_by(&mut self, delta_px: f32) {
        self.scroll_y = (self.scroll_y + delta_px).clamp(0.0, self.max_scroll_y());
    }

    /// 以光标为锚点缩放横向。
    pub fn zoom_x_at(&mut self, cursor_x: f32, new_zoom: f32) {
        let new_zoom = new_zoom.clamp(0.0002, 40.0);
        let tick = self.x_to_tick(cursor_x);
        self.zoom_x = new_zoom;
        let new_scroll = tick * new_zoom - (cursor_x - self.keyboard_width);
        let max = self.max_scroll_x();
        self.scroll_x = new_scroll.clamp(0.0, max);
        self.target_scroll_x = self.scroll_x;
    }

    /// 以光标为锚点缩放纵向。
    pub fn zoom_y_at(&mut self, cursor_y: f32, new_zoom: f32) {
        let new_zoom = new_zoom.clamp(4.0 * self.scale_factor, 60.0 * self.scale_factor);
        let key = self.y_to_key_f32(cursor_y);
        self.zoom_y = new_zoom;
        // 保持光标下的 key 不动
        self.scroll_y =
            ((self.key_count - 1) as f32 - key) * new_zoom - (cursor_y - self.ruler_height);
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
    }

    /// 纵向居中到指定 key。
    pub fn center_on_key(&mut self, key: u16) {
        let key = key.min(self.key_count - 1);
        self.scroll_y = ((self.key_count - 1 - key) as f32 * self.zoom_y
            - self.canvas_height() * 0.5)
            .clamp(0.0, self.max_scroll_y());
    }

    /// 动画推进；返回是否仍在移动。
    pub fn animate(&mut self, dt_secs: f32) -> bool {
        let diff = self.target_scroll_x - self.scroll_x;
        if diff.abs() < 0.05 {
            self.scroll_x = self.target_scroll_x;
            return false;
        }
        // 指数收敛：约 90ms 走完 80% 距离。
        let t = 1.0 - E.powf(-dt_secs * 18.0);
        self.scroll_x += diff * t;
        true
    }

    /// 窗口尺寸/DPI 变化。
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        self.width = width;
        self.height = height;
        self.scale_factor = scale_factor;
        self.keyboard_width = DEFAULT_KEYBOARD_WIDTH * scale_factor;
        self.ruler_height = DEFAULT_RULER_HEIGHT * scale_factor;
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x());
        self.target_scroll_x = self.target_scroll_x.clamp(0.0, self.max_scroll_x());
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
    }

    /// 当前帧是否处于滚动/缩放动画中（LOD 判据）。
    #[inline]
    #[must_use]
    pub fn is_moving(&self) -> bool {
        (self.target_scroll_x - self.scroll_x).abs() > 0.05
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        Camera::new(1600, 900, 1.0, 3_093_120, 1920, 0.1)
    }

    #[test]
    fn roundtrip_tick_x() {
        let mut c = camera();
        c.scroll_x = 123.0;
        c.target_scroll_x = 123.0;
        let x = c.tick_to_x(4567.0);
        assert!((c.x_to_tick(x) - 4567.0).abs() < 0.01);
    }

    #[test]
    fn roundtrip_key_y() {
        let c = camera();
        let y = c.key_to_y(60);
        assert_eq!(c.y_to_key(y), 60);
    }

    #[test]
    fn zoom_keeps_cursor_tick() {
        let mut c = camera();
        let x = 800.0;
        let before = c.x_to_tick(x);
        c.zoom_x_at(x, 1.0);
        let after = c.x_to_tick(x);
        assert!((before - after).abs() < 0.01, "{before} vs {after}");
    }

    #[test]
    fn visible_keys_are_ordered() {
        let c = camera();
        let (lo, hi) = c.visible_key_range();
        assert!(lo <= hi);
        assert!(hi <= 127);
    }
}
