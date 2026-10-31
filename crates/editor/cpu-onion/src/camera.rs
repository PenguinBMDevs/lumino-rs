//! 画布相机：只描述 canvas 区域，不含键盘栏 / 标尺 / 背景。
//!
//! 坐标口径与 GPU `onion_note.wgsl` 去掉 `keyboard_width` / `ruler_height`
//! 后一致：
//! - 横向：`x = tick * zoom_x - scroll_x`
//! - 纵向：`y = (key_count - 1 - key) * zoom_y - scroll_y`
//!
//! 本 crate 不知道文档总 tick 数，因此 [`CanvasCamera::tick_range_for_x`]
//! 只保证非负、`end > start` 且不溢出；调用方可在窗口扫描时用文档总 tick
//! 做额外裁剪（`ChunkedList::window_range` 对超出范围的 tick 本身是安全的）。

/// 画布相机（无键盘栏 / 标尺偏移）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasCamera {
    /// 画布宽度（像素）。
    pub width: u32,
    /// 画布高度（像素）。
    pub height: u32,
    /// 横向滚动（像素）。
    pub scroll_x: f32,
    /// 纵向滚动（像素，0 = 最顶部的 `key_count - 1` 键）。
    pub scroll_y: f32,
    /// 横向缩放（像素 / tick）。
    pub zoom_x: f32,
    /// 纵向缩放（像素 / key）。
    pub zoom_y: f32,
    /// 纵轴键数。
    pub key_count: u16,
}

impl CanvasCamera {
    /// 与主工程 `DEFAULT_ZOOM_Y` 一致的默认纵向缩放。
    pub const DEFAULT_ZOOM_Y: f32 = 20.0;
    /// 与主工程一致的默认键数。
    pub const DEFAULT_KEY_COUNT: u16 = 128;

    /// 创建相机：横向 1px/tick、纵向 20px/key、无滚动。
    #[must_use]
    pub const fn new(width: u32, height: u32, key_count: u16) -> Self {
        Self {
            width,
            height,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 1.0,
            zoom_y: Self::DEFAULT_ZOOM_Y,
            key_count,
        }
    }

    /// 画布高度（像素，浮点）。
    #[inline]
    #[must_use]
    pub fn canvas_height(&self) -> f32 {
        self.height as f32
    }

    /// 最大可见 key 下标（`key_count - 1`，空键位时饱和到 0）。
    #[inline]
    #[must_use]
    pub const fn max_key(&self) -> u16 {
        self.key_count.saturating_sub(1)
    }

    /// tick → 视口 x。
    #[inline]
    #[must_use]
    pub fn tick_to_x(&self, tick: f32) -> f32 {
        tick * self.zoom_x - self.scroll_x
    }

    /// 视口 x → tick。
    #[inline]
    #[must_use]
    pub fn x_to_tick(&self, x: f32) -> f32 {
        if self.zoom_x > 0.0 {
            (x + self.scroll_x) / self.zoom_x
        } else {
            0.0
        }
    }

    /// key → 视口 y。
    #[inline]
    #[must_use]
    pub fn key_to_y(&self, key: u16) -> f32 {
        self.max_key().saturating_sub(key) as f32 * self.zoom_y - self.scroll_y
    }

    /// 视口 y → key（浮点，未取整）。
    #[must_use]
    pub fn y_to_key_f32(&self, y: f32) -> f32 {
        if self.zoom_y <= 0.0 {
            return self.max_key() as f32;
        }
        self.max_key() as f32 - (y + self.scroll_y) / self.zoom_y
    }

    /// 可见 key 区间（含端点，已夹到 `[0, key_count - 1]`）。
    #[must_use]
    pub fn visible_key_range(&self) -> (u16, u16) {
        let max_key = self.max_key() as f32;
        let top = self.y_to_key_f32(0.0).ceil();
        let bottom = self.y_to_key_f32(self.canvas_height()).floor();
        let hi = top.clamp(0.0, max_key) as u16;
        let lo = bottom.clamp(0.0, max_key) as u16;
        (lo.min(hi), hi.max(lo))
    }

    /// 指定屏幕 x 区间的 tick 区间 `[start, end)`。
    ///
    /// band 渲染必须使用**本 band 的 x 区间**，否则每个 band 都会扫全视口。
    /// 本 crate 不知道文档总 tick 数，只保证非负、`end > start`（除非
    /// `start` 已饱和到 `u32::MAX`）且不溢出。
    #[must_use]
    pub fn tick_range_for_x(&self, x0: f32, x1: f32) -> (u32, u32) {
        if self.zoom_x <= 0.0 {
            return (0, 0);
        }
        let start = self.x_to_tick(x0).floor().max(0.0);
        let end = self.x_to_tick(x1).ceil().max(start + 1.0);
        let start = saturate_tick(start);
        let end = saturate_tick(end).max(start.saturating_add(1));
        (start, end)
    }

    /// 可见 tick 区间。
    #[must_use]
    pub fn visible_tick_range(&self) -> (u32, u32) {
        self.tick_range_for_x(0.0, self.width as f32)
    }
}

/// 浮点 tick 饱和到 `u32`（负数 / NaN → 0，超大值 → `u32::MAX`）。
#[inline]
fn saturate_tick(value: f32) -> u32 {
    if value.is_nan() {
        return 0;
    }
    value.clamp(0.0, u32::MAX as f32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> CanvasCamera {
        CanvasCamera {
            width: 800,
            height: 600,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 20.0,
            key_count: 128,
        }
    }

    #[test]
    fn roundtrip_tick_x() {
        let mut camera = camera();
        camera.scroll_x = 123.0;
        let x = camera.tick_to_x(4567.0);
        assert!((camera.x_to_tick(x) - 4567.0).abs() < 0.01);
    }

    #[test]
    fn roundtrip_key_y() {
        let camera = camera();
        for key in [0u16, 60, 127] {
            let y = camera.key_to_y(key) + camera.zoom_y * 0.25;
            assert_eq!(camera.y_to_key_f32(y).round() as u16, key);
        }
    }

    #[test]
    fn tick_range_covers_band() {
        let mut camera = camera();
        assert_eq!(camera.tick_range_for_x(0.0, 100.0), (0, 100));
        camera.scroll_x = 50.0;
        assert_eq!(camera.tick_range_for_x(0.0, 100.0), (50, 150));
        camera.zoom_x = 2.0;
        assert_eq!(camera.tick_range_for_x(0.0, 100.0), (25, 75));
    }

    #[test]
    fn zero_zoom_is_safe() {
        let mut camera = camera();
        camera.zoom_x = 0.0;
        camera.zoom_y = 0.0;
        assert_eq!(camera.tick_range_for_x(0.0, 100.0), (0, 0));
        assert!(camera.key_to_y(60).is_finite());
        let (lo, hi) = camera.visible_key_range();
        assert_eq!((lo, hi), (127, 127));
    }

    #[test]
    fn new_camera_defaults() {
        let camera = CanvasCamera::new(100, 50, 128);
        assert_eq!(camera.zoom_x, 1.0);
        assert_eq!(camera.zoom_y, CanvasCamera::DEFAULT_ZOOM_Y);
        assert_eq!(camera.max_key(), 127);
        assert_eq!(camera.visible_tick_range(), (0, 100));
    }

    #[test]
    fn visible_keys_are_bounded() {
        let camera = camera();
        let (lo, hi) = camera.visible_key_range();
        assert!(lo <= hi);
        assert!(hi <= 127);
    }
}
