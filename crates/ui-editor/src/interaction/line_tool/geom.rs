//! 曲线工具贝塞尔几何算法（纯函数，无 Editor 依赖）
//!
//! 包含：三次贝塞尔求值、点到曲线距离、曲线/直线格点离散化。

use iced_core::Point;
use lumino_editor_state::BezierAnchor;

/// 点到线段的最短距离
pub(super) fn point_segment_distance(p: Point, a: Point, b: Point) -> f32 {
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
pub(super) fn bezier_point(
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
pub(super) fn point_curve_distance(p: Point, a: Point, p1: Point, p2: Point, b: Point) -> f32 {
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

/// 贝塞尔曲线离散化：生成曲线经过的所有网格格点
///
/// - 两端控制柄均为自动维护（未自定义，柄 = 段方向 1/3 = 精确直线）：
///   走 Bresenham 精确格点（历史行为一致）；
/// - 任一端为自定义弯曲柄：按 `snap`（tick 方向）/ 1（key 方向）分格采样，
///   采样数 = max(tick 格数, key 格数) × 4（过采样保证每格至少命中，
///   竖直段不漏格）；结果按路径顺序去重。
pub(super) fn curve_cell_points(a: BezierAnchor, b: BezierAnchor, snap: f32) -> Vec<(f32, u16)> {
    // 自动柄（未弯曲）= 贝塞尔直线段 → Bresenham 精确格点
    if a.handles_auto && b.handles_auto {
        return line_cell_points(a.pos, b.pos, snap);
    }
    let snap = snap.max(1.0);
    let tick_cells = ((b.pos.0 - a.pos.0).abs() / snap).ceil() as usize;
    let key_cells = (b.pos.1 - a.pos.1).abs().ceil() as usize;
    let n = (tick_cells.max(key_cells) * 4).max(1);
    let cp1 = a.out_handle_abs();
    let cp2 = b.in_handle_abs();
    let mut points = Vec::with_capacity(n + 1);
    let mut last: Option<(f32, u16)> = None;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let (x, y) = bezier_point(a.pos, cp1, cp2, b.pos, t);
        let tick = (x / snap).round() * snap;
        let key = y.round().clamp(0.0, 255.0) as u16;
        let p = (tick, key);
        if last != Some(p) {
            points.push(p);
            last = Some(p);
        }
    }
    points
}

/// Bresenham 直线算法：生成直线经过的所有网格格点
///
/// tick 方向按 `snap` 分格，key 方向每个 key 一格；
/// 结果按路径顺序排列（tick/key 单调，无重复格点）。
pub(super) fn line_cell_points(a: (f32, f32), b: (f32, f32), snap: f32) -> Vec<(f32, u16)> {
    let snap = snap.max(1.0);
    let x0 = (a.0 / snap).round() as i64;
    let y0 = a.1.round() as i64;
    let x1 = (b.0 / snap).round() as i64;
    let y1 = b.1.round() as i64;
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;
    let mut points = Vec::with_capacity((dx + dy + 1) as usize);
    loop {
        points.push((x as f32 * snap, y as u16));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
    points
}
