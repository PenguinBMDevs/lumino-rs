//! 凹多边形三角化（耳切法）。
//!
//! egui 的 `PathShape` 填充只做**凸多边形**扇面三角化，凹闭环会填出多边形外的
//! 三角面；曲线工具的内部填充闭环可以是任意简单多边形，因此在这里先切耳，
//! 再把三角形交给 `Mesh` 绘制（等价于旧实现 NonZero 填充的可见结果）。

use egui::Pos2;

/// 三角形顶点是输入点数组的下标。
type Tri = [u32; 3];

/// 面积叉积 `(b-a) × (c-a)` 的 z 分量。
fn cross(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// 点是否在三角形内（含边界，要求三角形逆时针）。
fn point_in_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    cross(a, b, p) >= -1e-6 && cross(b, c, p) >= -1e-6 && cross(c, a, p) >= -1e-6
}

/// 把闭环切分成三角形（下标指向输入点）。
///
/// 输入按任意方向给出；内部统一为逆时针后切耳。自交或退化多边形返回已切出的
/// 部分（可能为空），调用方应据此退回只画描边，避免填出色块。
pub(super) fn ear_clip(points: &[Pos2]) -> Vec<Tri> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }

    let mut order: Vec<usize> = (0..n).collect();
    let area2: f32 = (0..n)
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % n];
            a.x * b.y - b.x * a.y
        })
        .sum();
    if area2.abs() < 1e-9 {
        return Vec::new();
    }
    // 统一为逆时针：凸顶点叉积为正，耳切判据才成立。
    if area2 < 0.0 {
        order.reverse();
    }

    let mut out: Vec<Tri> = Vec::with_capacity(n.saturating_sub(2));
    let mut guard = 0usize;
    while order.len() > 3 {
        let m = order.len();
        let mut clipped = false;
        guard += 1;
        if guard > n * n {
            break;
        }
        for i in 0..m {
            let i0 = order[(i + m - 1) % m];
            let i1 = order[i];
            let i2 = order[(i + 1) % m];
            let (a, b, c) = (points[i0], points[i1], points[i2]);

            // 凹点（或共线）不能切。
            if cross(a, b, c) <= 1e-6 {
                continue;
            }
            // 其它顶点落在三角形内 → 不是耳。
            let blocked = order
                .iter()
                .any(|&j| j != i0 && j != i1 && j != i2 && point_in_triangle(points[j], a, b, c));
            if blocked {
                continue;
            }

            out.push([i0 as u32, i1 as u32, i2 as u32]);
            order.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }

    if order.len() == 3 {
        let (a, b, c) = (order[0], order[1], order[2]);
        if cross(points[a], points[b], points[c]) > 1e-6 {
            out.push([a as u32, b as u32, c as u32]);
        }
    }

    out
}

/// 三角形面积之和（仅测试用于与多边形面积对照）。
#[cfg(test)]
fn triangle_area_sum(points: &[Pos2], tris: &[Tri]) -> f32 {
    tris.iter()
        .map(|&[a, b, c]| {
            cross(points[a as usize], points[b as usize], points[c as usize]).abs() * 0.5
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Pos2 {
        Pos2::new(x, y)
    }

    #[test]
    fn convex_quad_yields_two_triangles() {
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
        let tris = ear_clip(&pts);
        assert_eq!(tris.len(), 2);
        assert!((triangle_area_sum(&pts, &tris) - 100.0).abs() < 1e-3);
    }

    /// 射线法判断点是否在简单多边形内（仅测试用）。
    fn point_in_polygon(poly: &[Pos2], p: Pos2) -> bool {
        let mut inside = false;
        let n = poly.len();
        for i in 0..n {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            let straddles = (a.y > p.y) != (b.y > p.y);
            if straddles {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if p.x < x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// L 形凹多边形：切耳后的三角形总面积必须等于多边形面积，
    /// 且每个三角形重心都落在多边形内（凸扇面会溢出到缺口）。
    #[test]
    fn concave_l_shape_stays_inside() {
        let pts = [
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 4.0),
            p(4.0, 4.0),
            p(4.0, 10.0),
            p(0.0, 10.0),
        ];
        let tris = ear_clip(&pts);
        assert_eq!(tris.len(), 4, "6 边形应切成 4 个三角形");
        let area = 10.0 * 4.0 + 4.0 * 6.0;
        assert!((triangle_area_sum(&pts, &tris) - area).abs() < 1e-3);

        // 缺口区域 (x>4 && y>4) 在多边形外：任何三角形重心都不允许落在那里。
        for &[a, b, c] in &tris {
            let (a, b, c) = (pts[a as usize], pts[b as usize], pts[c as usize]);
            let center = p((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
            assert!(
                point_in_polygon(&pts, center),
                "三角形重心 {center:?} 落到了凹多边形之外"
            );
        }
    }

    #[test]
    fn three_points_is_one_triangle() {
        let pts = [p(0.0, 0.0), p(4.0, 0.0), p(0.0, 3.0)];
        assert_eq!(ear_clip(&pts).len(), 1);
    }

    #[test]
    fn degenerate_input_is_empty() {
        assert!(ear_clip(&[]).is_empty());
        assert!(ear_clip(&[p(0.0, 0.0), p(1.0, 1.0)]).is_empty());
        assert!(
            ear_clip(&[p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0)]).is_empty(),
            "共线三点没有可填充面积"
        );
    }
}
