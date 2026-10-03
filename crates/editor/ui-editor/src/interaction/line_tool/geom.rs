//! 曲线工具贝塞尔几何算法（纯函数，无 Editor 依赖）
//!
//! 包含：三次贝塞尔求值、点到曲线距离、贝塞尔**展平**（路径 → 折线点列，
//! 供 [`super::paths`] 的逐音高行跨越计算使用）。

use iced_core::Point;
use lumino_editor_state::BezierAnchor;

/// tick 方向展平容差（tick）：音符起点四舍五入到整 tick，取远小于半 tick 的值
const TICK_TOL: f64 = 0.1;
/// key 方向展平容差（key）：远小于半行（0.5），不会误判音高行
const KEY_TOL: f64 = 0.02;
/// 单段最少 / 最多细分段数（上限约束极端数据的代价）
const MIN_STEPS: usize = 4;
const MAX_STEPS: usize = 2048;

/// 点到线段的最短距离
pub(crate) fn point_segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let ab_x = b.x - a.x;
    let ab_y = b.y - a.y;
    let len_sq = ab_x * ab_x + ab_y * ab_y;
    if len_sq <= f32::EPSILON {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * ab_x + (p.y - a.y) * ab_y) / len_sq).clamp(0.0, 1.0);
    let proj_x = a.x + t * ab_x;
    let proj_y = a.y + t * ab_y;
    (p.x - proj_x).hypot(p.y - proj_y)
}

/// 三次贝塞尔曲线点（多项式形式）
pub(crate) fn bezier_point(
    a: (f32, f32),
    cp1: (f32, f32),
    cp2: (f32, f32),
    b: (f32, f32),
    t: f32,
) -> (f32, f32) {
    let u = 1.0 - t;
    let x = u * u * u * a.0 + 3.0 * u * u * t * cp1.0 + 3.0 * u * t * t * cp2.0 + t * t * t * b.0;
    let y = u * u * u * a.1 + 3.0 * u * u * t * cp1.1 + 3.0 * u * t * t * cp2.1 + t * t * t * b.1;
    (x, y)
}

/// 点到贝塞尔曲线的近似距离（16 段折线逼近）
pub(crate) fn point_curve_distance(p: Point, a: Point, p1: Point, p2: Point, b: Point) -> f32 {
    const SAMPLES: usize = 16;
    let mut min = f32::INFINITY;
    let mut prev = a;
    for i in 1..=SAMPLES {
        let t = i as f32 / SAMPLES as f32;
        let (x, y) = bezier_point((a.x, a.y), (p1.x, p1.y), (p2.x, p2.y), (b.x, b.y), t);
        let cur = Point::new(x, y);
        min = min.min(point_segment_distance(p, prev, cur));
        prev = cur;
    }
    min
}

/// 贝塞尔路径展平为 (tick, key) 折线点列（供逐音高行跨越计算使用）。
///
/// **不使用任何「设定精度」网格**：细分段数由曲线在**几何容差**
/// （[`TICK_TOL`] / [`KEY_TOL`]）下的尺度推出——三次贝塞尔均匀细分时折线
/// 与曲线的偏差不超过 `0.75 · D / n²`（`D` = 控制多边形二阶差分），
/// 取 `n = √(D / 容差)` 已留余量；容差只描述几何误差，与用户的吸附精度无关。
///
/// - 自动柄段（未弯曲：`cp1 = A + (B - A)/3`、`cp2 = B - (B - A)/3`）是**精确直线**
///   → 只取两端点，不做任何细分；
/// - 自定义柄段按斜率修正后的 key 容差细分：平坦段上 key 的一点点误差会被
///   放大成很大的 tick 误差（`Δtick = Δkey / 斜率`），容差必须随之收紧。
///
/// 全程 **f64**：`paths` 里的「行内微缩」（`EDGE = 0.5 - 1e-6`）在 f32 下会被
/// 舍入吃掉——key ≈ 64 处 f32 间距约 3.8e-6，`64.499999` 舍成 `64.5`，音高行
/// 会凭空多出一行（Spiderweb 用 numpy 的 float64，同此）。
///
/// 相邻重复点不入列（竖直退化段只留一个点，与贝塞尔采样一致）。
pub(crate) fn flatten_path(path: &[BezierAnchor]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(path.len() * 8 + 1);
    let Some(first) = path.first() else {
        return out;
    };
    out.push((first.pos.0 as f64, first.pos.1 as f64));
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let p0 = (a.pos.0 as f64, a.pos.1 as f64);
        let p3 = (b.pos.0 as f64, b.pos.1 as f64);
        if a.handles_auto && b.handles_auto {
            push_point(&mut out, p3);
            continue;
        }
        let h1 = a.out_handle_abs();
        let h2 = b.in_handle_abs();
        let p1 = (h1.0 as f64, h1.1 as f64);
        let p2 = (h2.0 as f64, h2.1 as f64);
        let steps = segment_steps(p0, p1, p2, p3);
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            push_point(&mut out, bezier_point64(p0, p1, p2, p3, t));
        }
    }
    out
}

/// 三次贝塞尔曲线点（f64，展平专用）
fn bezier_point64(
    a: (f64, f64),
    cp1: (f64, f64),
    cp2: (f64, f64),
    b: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    let x = u * u * u * a.0 + 3.0 * u * u * t * cp1.0 + 3.0 * u * t * t * cp2.0 + t * t * t * b.0;
    let y = u * u * u * a.1 + 3.0 * u * u * t * cp1.1 + 3.0 * u * t * t * cp2.1 + t * t * t * b.1;
    (x, y)
}

/// 相邻重复点不入列
fn push_point(out: &mut Vec<(f64, f64)>, p: (f64, f64)) {
    if out.last() != Some(&p) {
        out.push(p);
    }
}

/// 单段贝塞尔细分的段数：`n = √(D / 容差)`，`D` = 各轴二阶差分归一化到容差后的最大值。
fn segment_steps(a: (f64, f64), cp1: (f64, f64), cp2: (f64, f64), b: (f64, f64)) -> usize {
    let slope = control_slope(a, cp1, cp2, b);
    let key_tol = if slope.is_finite() && slope > 0.0 {
        KEY_TOL.min(TICK_TOL * slope)
    } else {
        KEY_TOL
    };
    let second =
        |p: f64, q: f64, r: f64, s: f64| (p - 2.0 * q + r).abs().max((q - 2.0 * r + s).abs());
    let dx = second(a.0, cp1.0, cp2.0, b.0) / TICK_TOL;
    let dy = second(a.1, cp1.1, cp2.1, b.1) / key_tol;
    let scale = dx.max(dy);
    if !scale.is_finite() || scale <= 0.0 {
        return MIN_STEPS;
    }
    ((scale.sqrt().ceil() as usize) + 2).clamp(MIN_STEPS, MAX_STEPS)
}

/// 控制多边形各边的最大斜率 |Δkey| / |Δtick|（曲线最大斜率的代理上界）；
/// 全部边都竖直时退回弦的斜率。
fn control_slope(a: (f64, f64), cp1: (f64, f64), cp2: (f64, f64), b: (f64, f64)) -> f64 {
    let mut slope: f64 = 0.0;
    for (p, q) in [(a, cp1), (cp1, cp2), (cp2, b)] {
        let dx = (q.0 - p.0).abs();
        if dx > 1e-9 {
            slope = slope.max((q.1 - p.1).abs() / dx);
        }
    }
    if slope > 0.0 {
        slope
    } else {
        (b.1 - a.1).abs() / (b.0 - a.0).abs().max(1e-9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_editor_state::BezierAnchor;

    #[test]
    fn test_flatten_straight_is_two_points() {
        // 自动柄 = 精确直线 → 只取两端点（不做任何按精度的格点采样）
        let a = BezierAnchor::new((0.0, 60.0));
        let b = BezierAnchor::new((3840.0, 60.0));
        assert_eq!(flatten_path(&[a, b]), vec![(0.0, 60.0), (3840.0, 60.0)]);
    }

    #[test]
    fn test_flatten_diagonal_straight_is_two_points() {
        // 斜直线同样只取两端点：音符边界由后续解析求交决定，不靠采样
        let a = BezierAnchor::new((0.0, 60.0));
        let b = BezierAnchor::new((3840.0, 64.0));
        assert_eq!(flatten_path(&[a, b]), vec![(0.0, 60.0), (3840.0, 64.0)]);
    }

    #[test]
    fn test_flatten_bent_curve_reaches_high_keys() {
        // 弯曲曲线：出向柄拉高 → 折线经过中间更高的 key，首尾仍是锚点
        let mut a = BezierAnchor::new((0.0, 60.0));
        a.set_out_handle((960.0, 20.0));
        let mut b = BezierAnchor::new((1920.0, 60.0));
        b.set_in_handle((-960.0, 20.0));
        let pts = flatten_path(&[a, b]);
        assert_eq!(pts.first(), Some(&(0.0, 60.0)));
        assert_eq!(pts.last(), Some(&(1920.0, 60.0)));
        assert!(pts.iter().any(|&(_, k)| k > 60.0), "曲线应经过更高 key");
    }

    /// 展平精度：折线在 key 方向的逼近误差必须远小于半行（否则音高行会判错）。
    ///
    /// 注意判据不是"相邻点 key 差 < 1"——`parts_notes` 对一段跨越多个音高行的
    /// 折线段是**解析展开**的（逐行插值求交点），步长跨度大本身不会漏行；
    /// 真正影响正确性的是折线离真实曲线的偏差。
    #[test]
    fn test_flatten_stays_close_to_the_curve() {
        let mut a = BezierAnchor::new((0.0, 60.0));
        a.set_out_handle((2880.0, 30.0));
        let mut b = BezierAnchor::new((5760.0, 60.0));
        b.set_in_handle((-2880.0, 30.0));
        let p0 = (a.pos.0 as f64, a.pos.1 as f64);
        let h1 = a.out_handle_abs();
        let h2 = b.in_handle_abs();
        let p1 = (h1.0 as f64, h1.1 as f64);
        let p2 = (h2.0 as f64, h2.1 as f64);
        let p3 = (b.pos.0 as f64, b.pos.1 as f64);
        let pts = flatten_path(&[a, b]);
        assert!(pts.len() > 16, "弯曲段必须细分，实际 {} 点", pts.len());
        let mut worst = 0.0f64;
        for i in 0..=2000 {
            let t = i as f64 / 2000.0;
            let (x, y) = bezier_point64(p0, p1, p2, p3, t);
            for w in pts.windows(2) {
                let (xa, ya) = w[0];
                let (xb, yb) = w[1];
                let inside = (xa <= x && x <= xb) || (xb <= x && x <= xa);
                if !inside && (xa - x).abs() > 1e-9 {
                    continue;
                }
                let u = if (xb - xa).abs() < 1e-12 {
                    0.0
                } else {
                    (x - xa) / (xb - xa)
                };
                worst = worst.max((ya + (yb - ya) * u - y).abs());
                break;
            }
        }
        assert!(
            worst < KEY_TOL,
            "折线 key 偏差 {worst} 必须小于 {KEY_TOL}（半行的 1/25）"
        );
    }

    /// 竖直弯折段（tick 不动）不得退化成单点：否则整段音高行丢失
    #[test]
    fn test_flatten_vertical_bend_keeps_rows() {
        let mut a = BezierAnchor::new((1920.0, 60.0));
        a.set_out_handle((0.0, 8.0));
        let mut b = BezierAnchor::new((1920.0, 68.0));
        b.set_in_handle((0.0, -8.0));
        let pts = flatten_path(&[a, b]);
        assert!(pts.len() > 2, "竖直弯曲段必须细分，实际 {} 点", pts.len());
        assert_eq!(pts.first(), Some(&(1920.0, 60.0)));
        assert_eq!(pts.last(), Some(&(1920.0, 68.0)));
    }

    #[test]
    fn test_point_curve_distance() {
        // 直线退化：点到曲线距离 ≈ 点到线段距离
        let a = Point::new(0.0, 0.0);
        let p1 = Point::new(10.0, 0.0);
        let p2 = Point::new(20.0, 0.0);
        let b = Point::new(30.0, 0.0);
        let d = point_curve_distance(Point::new(15.0, 5.0), a, p1, p2, b);
        assert!((d - 5.0).abs() < 0.1, "直线退化距离应接近 5，实际 {d}");
    }
}
