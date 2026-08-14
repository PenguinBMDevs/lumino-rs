//! 滚轮缩放公共逻辑 —— 钢琴卷帘与工程走带共用
//!
//! 统一 Ctrl+滚轮缩放的行为约定（模仿钢琴卷帘的键盘控制缩放逻辑）：
//! - 缩放因子按滚轮刻度平滑步进：每滚动一个刻度（line），缩放倍率变化 10%，
//!   Pixel 增量先换算为刻度线再参与计算，防止高精度触控板单次跳变过大；
//! - 缩放锚点为鼠标指针在视口内的相对位置（0.0 贴左/上，1.0 贴右/下），
//!   缩放后指针下的音乐坐标（tick/音轨）保持不动。

use iced_core::mouse::ScrollDelta;

/// 滚轮缩放步进系数：每滚动一个刻度（line），缩放倍率变化 10%
pub const ZOOM_WHEEL_STEP: f32 = 0.1;
/// Pixel 增量换算为刻度线的除数（与力度面板 Automation 缩放的换算保持一致）
pub const PIXEL_TO_LINE_SCALE: f32 = 50.0;

/// 计算缩放因子：向上滚动（delta > 0）放大、向下滚动（delta < 0）缩小。
///
/// 返回 `None` 表示无需缩放（增量为 0）。
pub fn zoom_factor_from_delta(delta: &ScrollDelta) -> Option<f32> {
    let line_delta = match delta {
        ScrollDelta::Lines { y, .. } => *y,
        ScrollDelta::Pixels { y, .. } => *y / PIXEL_TO_LINE_SCALE,
    };
    let step = line_delta.clamp(-1.0, 1.0);
    if step.abs() < f32::EPSILON {
        None
    } else {
        Some(1.0 + step * ZOOM_WHEEL_STEP)
    }
}

/// 计算鼠标在视口内的锚点比例（0.0 贴左/上，1.0 贴右/下）。
///
/// 视口尺寸过小时回退到中心锚点（0.5）。
pub fn fixed_ratio_from_viewport(position: f32, origin: f32, viewport_size: f32) -> f32 {
    if viewport_size > 0.0 {
        ((position - origin) / viewport_size).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_factor_zoom_in_on_scroll_up() {
        // 向上滚动（y > 0）→ 放大
        let factor = zoom_factor_from_delta(&ScrollDelta::Lines { x: 0.0, y: 1.0 });
        assert_eq!(factor, Some(1.1));
    }

    #[test]
    fn test_zoom_factor_zoom_out_on_scroll_down() {
        // 向下滚动（y < 0）→ 缩小
        let factor = zoom_factor_from_delta(&ScrollDelta::Lines { x: 0.0, y: -1.0 });
        assert_eq!(factor, Some(0.9));
    }

    #[test]
    fn test_zoom_factor_zero_delta_returns_none() {
        assert_eq!(
            zoom_factor_from_delta(&ScrollDelta::Lines { x: 0.0, y: 0.0 }),
            None
        );
    }

    #[test]
    fn test_zoom_factor_pixels_converted_and_clamped() {
        // 像素增量换算：y=50 → 1 个刻度
        let factor = zoom_factor_from_delta(&ScrollDelta::Pixels { x: 0.0, y: 50.0 });
        assert_eq!(factor, Some(1.1));
        // 大增量被钳制为单个刻度（单步缩放，防止跳变）
        let factor = zoom_factor_from_delta(&ScrollDelta::Pixels { x: 0.0, y: -500.0 });
        assert_eq!(factor, Some(0.9));
    }

    #[test]
    fn test_fixed_ratio_from_viewport() {
        // 锚点比例：视口 [60, 800) 内，60 → 0.0（贴左/上），430 → 0.5（中心），800 → 1.0（贴右/下）
        let ratio = fixed_ratio_from_viewport(60.0, 60.0, 740.0);
        assert!((ratio - 0.0).abs() < f32::EPSILON);
        let ratio = fixed_ratio_from_viewport(430.0, 60.0, 740.0);
        assert!((ratio - 0.5).abs() < f32::EPSILON);
        let ratio = fixed_ratio_from_viewport(800.0, 60.0, 740.0);
        assert!((ratio - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_fixed_ratio_from_viewport_degenerate_falls_back_to_center() {
        // 视口退化（尺寸为 0）时回退到中心锚点
        let ratio = fixed_ratio_from_viewport(0.0, 0.0, 0.0);
        assert_eq!(ratio, 0.5);
    }
}
