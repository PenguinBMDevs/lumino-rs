//! 形状插入的比例适配（DRAW-033B2a）

use lumino_core::shape::ShapeRect;

/// 把 source 按原比例缩放到 target 内并居中（Contain）。
///
/// source/target 都是 tick/key 矩形；source 宽高任一非正时退回 target。
pub(crate) fn fit_contain_rect(source: ShapeRect, target: ShapeRect) -> ShapeRect {
    let (sw, sh) = source.size();
    let (tw, th) = target.size();
    if sw <= 0.0 || sh <= 0.0 || tw <= 0.0 || th <= 0.0 {
        return target;
    }
    let scale = (tw / sw).min(th / sh);
    let width = sw * scale;
    let height = sh * scale;
    let cx = (target.tick_lo + target.tick_hi) * 0.5;
    let cy = (target.key_lo + target.key_hi) * 0.5;
    ShapeRect::normalized(
        cx - width * 0.5,
        cy - height * 0.5,
        cx + width * 0.5,
        cy + height * 0.5,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> ShapeRect {
        ShapeRect::normalized(x0, y0, x1, y1)
    }

    #[test]
    fn test_contain_keeps_aspect_and_centers() {
        // source 2:1，target 4:4 → 按宽适配，得到 4:2 居中。
        let out = fit_contain_rect(rect(0.0, 0.0, 2.0, 1.0), rect(0.0, 0.0, 4.0, 4.0));
        let (w, h) = out.size();
        assert!((w - 4.0).abs() < 1e-5);
        assert!((h - 2.0).abs() < 1e-5);
        assert!((out.key_lo - 1.0).abs() < 1e-5);
        assert!((out.key_hi - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_contain_uses_height_when_target_is_wide() {
        // source 1:2，target 8:4 → 按高适配，得到 2:4 居中。
        let out = fit_contain_rect(rect(0.0, 0.0, 1.0, 2.0), rect(0.0, 0.0, 8.0, 4.0));
        let (w, h) = out.size();
        assert!((w - 2.0).abs() < 1e-5);
        assert!((h - 4.0).abs() < 1e-5);
        assert!((out.tick_lo - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_contain_invalid_source_returns_target() {
        let target = rect(0.0, 0.0, 4.0, 4.0);
        assert_eq!(fit_contain_rect(rect(0.0, 0.0, 0.0, 1.0), target), target);
    }
}
