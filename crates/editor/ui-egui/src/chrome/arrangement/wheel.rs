//! 走带滚轮手势解析（泳道 / 轨道列共用）。
//!
//! egui 把一帧内的滚轮事件放在 `InputState::raw.events` 里。Windows 会把
//! 触控板双指斜向滑动拆成两条独立事件（`WM_MOUSEWHEEL` 只带 y、
//! `WM_MOUSEHWHEEL` 只带 x），因此必须**累加本帧全部事件**，不能只取最后
//! 一条——否则后到的轴会覆盖先到的轴，斜向滚动退化成单轴。
//!
//! 方向与钢琴卷帘 `handle_wheel_scroll` / `handle_scrolled` 同口径：
//! - Line 刻度按 [`LINES_TO_PIXELS`] 换算成像素（等价 `SCROLL_LINES_SCALE`）；
//! - Shift 且平台未给出横向分量时，把纵向增量转成横向；
//! - 滚动方向为 `scroll = base - delta`（内容跟随手指）。

use egui::Vec2;

/// 一个滚轮刻度对应的像素（与 `SCROLL_LINES_SCALE` 一致）。
const LINES_TO_PIXELS: f32 = 30.0;

/// Pixel 增量换算成刻度线的除数（与 `zoom::PIXEL_TO_LINE_SCALE` 一致）。
const PIXELS_TO_LINES: f32 = 50.0;

/// 一帧内所有滚轮事件的累加结果。
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct WheelGesture {
    /// 以「行」为单位的事件（Line 原样、Page 折算成 10 行）。
    lines: Vec2,
    /// 以像素为单位的事件（Point = 触控板高精度增量）。
    pixels: Vec2,
}

impl WheelGesture {
    /// 读取本帧全部滚轮事件并累加。
    ///
    /// 返回 `None` 表示本帧没有滚轮事件；有事件但增量为 0 时返回空手势，
    /// 调用方按各轴增量是否为 0 决定是否发动作。
    pub(super) fn collect(ui: &egui::Ui) -> Option<Self> {
        let mut gesture = Self::default();
        let mut found = false;
        ui.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::MouseWheel { unit, delta, .. } = event {
                    found = true;
                    match unit {
                        egui::MouseWheelUnit::Line => gesture.lines += *delta,
                        egui::MouseWheelUnit::Point => gesture.pixels += *delta,
                        egui::MouseWheelUnit::Page => gesture.lines += *delta * 10.0,
                    }
                }
            }
        });
        found.then_some(gesture)
    }

    /// 本帧滚动增量（像素）。
    pub(super) fn scroll_pixels(self) -> Vec2 {
        self.lines * LINES_TO_PIXELS + self.pixels
    }

    /// 本帧纵向缩放手势（刻度线，未钳制）。
    pub(super) fn zoom_lines(self) -> f32 {
        self.lines.y + self.pixels.y / PIXELS_TO_LINES
    }

    /// 滚动目标位置：`(scroll_x, scroll_y)`，`None` = 该轴本帧无增量。
    ///
    /// 与钢琴卷帘 `handle_wheel_scroll` 同规则：Shift 且平台没有给出横向
    /// 分量时，把纵向增量整体转成横向（平台已转换时不重复处理）。
    /// 方向为 `target = base - delta`，触控板左滑 / 滚轮向下都会让视图右移。
    pub(super) fn scroll_targets(
        self,
        shift: bool,
        scroll_x: f32,
        scroll_y: f32,
    ) -> (Option<f32>, Option<f32>) {
        let mut delta = self.scroll_pixels();
        if shift && delta.x.abs() < f32::EPSILON {
            delta.x = delta.y;
            delta.y = 0.0;
        }
        (
            (delta.x.abs() > f32::EPSILON).then(|| (scroll_x - delta.x).max(0.0)),
            (delta.y.abs() > f32::EPSILON).then(|| (scroll_y - delta.y).max(0.0)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_pixels_scales_lines_and_keeps_pixels() {
        let gesture = WheelGesture {
            lines: Vec2::new(1.0, -2.0),
            pixels: Vec2::new(10.0, 5.0),
        };
        // 1 行 → 30px，-2 行 → -60px；Point 原样叠加。
        assert_eq!(gesture.scroll_pixels(), Vec2::new(40.0, -55.0));
    }

    #[test]
    fn zoom_lines_only_uses_vertical_axis() {
        let gesture = WheelGesture {
            lines: Vec2::new(99.0, 1.0),
            pixels: Vec2::new(99.0, -50.0),
        };
        // 横向分量不参与缩放；1 行 + (-50px / 50) = 0 个刻度。
        assert_eq!(gesture.zoom_lines(), 0.0);
    }

    #[test]
    fn shift_maps_vertical_wheel_to_horizontal() {
        // 滚轮向下（-1 行 → -30px）→ 横向 delta = -30 →
        // target = 100 - (-30) = 130（视图右移，与钢琴卷帘一致）。
        let gesture = WheelGesture {
            lines: Vec2::new(0.0, -1.0),
            pixels: Vec2::ZERO,
        };
        let (x, y) = gesture.scroll_targets(true, 100.0, 50.0);
        assert_eq!(x, Some(130.0));
        assert_eq!(y, None);
    }

    #[test]
    fn diagonal_wheel_scrolls_both_axes() {
        // 触控板左滑 + 上滑：dx<0、dy<0 → 两轴目标都增大。
        let gesture = WheelGesture {
            lines: Vec2::ZERO,
            pixels: Vec2::new(-100.0, -50.0),
        };
        let (x, y) = gesture.scroll_targets(false, 100.0, 50.0);
        assert_eq!(x, Some(200.0));
        assert_eq!(y, Some(100.0));
    }

    #[test]
    fn platform_converted_shift_keeps_both_axes() {
        // 平台已经给出横向分量（x != 0）时不再把 y 转 x。
        let gesture = WheelGesture {
            lines: Vec2::new(-1.0, -1.0),
            pixels: Vec2::ZERO,
        };
        let (x, y) = gesture.scroll_targets(true, 0.0, 0.0);
        assert_eq!(x, Some(30.0));
        assert_eq!(y, Some(30.0));
    }

    #[test]
    fn zero_delta_yields_no_targets() {
        let gesture = WheelGesture::default();
        assert_eq!(gesture.scroll_targets(true, 10.0, 20.0), (None, None));
    }
}
