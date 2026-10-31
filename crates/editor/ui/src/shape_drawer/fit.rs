//! 形状插入的比例适配（DRAW-033B2a）

use lumino_core::shape::ShapeRect;

/// 把 source 按原比例缩放到 target 内并居中（Contain）。
///
/// source 是抽屉单位框坐标；target 的 x 轴是 tick，需用 `ppq` 换算成拍后
/// 再与 key 轴比较比例（不能直接拿 tick 与 key 比，否则 Contain 会缩成一条细线）。
pub(crate) fn fit_contain_rect(source: ShapeRect, target: ShapeRect, ppq: f32) -> ShapeRect {
    let (sw, sh) = source.size();
    let (tw, th) = target.size();
    if sw <= 0.0 || sh <= 0.0 || tw <= 0.0 || th <= 0.0 || !ppq.is_finite() || ppq <= 0.0 {
        return target;
    }
    let source_aspect = sw / sh;
    let target_beats = tw / ppq;
    let target_aspect = target_beats / th;
    let (width_ticks, height_keys) = if source_aspect > target_aspect {
        (tw, target_beats / source_aspect)
    } else {
        (th * source_aspect * ppq, th)
    };
    let cx = (target.tick_lo + target.tick_hi) * 0.5;
    let cy = (target.key_lo + target.key_hi) * 0.5;
    ShapeRect::normalized(
        cx - width_ticks * 0.5,
        cy - height_keys * 0.5,
        cx + width_ticks * 0.5,
        cy + height_keys * 0.5,
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
        let out = fit_contain_rect(rect(0.0, 0.0, 2.0, 1.0), rect(0.0, 0.0, 4.0, 4.0), 1.0);
        let (w, h) = out.size();
        assert!((w - 4.0).abs() < 1e-5);
        assert!((h - 2.0).abs() < 1e-5);
        assert!((out.key_lo - 1.0).abs() < 1e-5);
        assert!((out.key_hi - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_contain_uses_height_when_target_is_wide() {
        // source 1:2，target 8:4 → 按高适配，得到 2:4 居中。
        let out = fit_contain_rect(rect(0.0, 0.0, 1.0, 2.0), rect(0.0, 0.0, 8.0, 4.0), 1.0);
        let (w, h) = out.size();
        assert!((w - 2.0).abs() < 1e-5);
        assert!((h - 4.0).abs() < 1e-5);
        assert!((out.tick_lo - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_contain_invalid_source_returns_target() {
        let target = rect(0.0, 0.0, 4.0, 4.0);
        assert_eq!(
            fit_contain_rect(rect(0.0, 0.0, 0.0, 1.0), target, 1.0),
            target
        );
    }
}
