//! 渲染层闭环几何：把填充边组装成闭环、环内点判定
//!
//! 填充的**语义计算**（点击判定、BFS、√ 音符生成）仍由 `fill_cells` 完成；
//! 本模块为**矢量渲染**服务：填充的视觉边缘必须与曲线几何一致
//! （而不是 snap/key 网格的锯齿格点），因此把 `collect_edges` 的边
//! 组装成闭环后直接作为填充路径绘制。

use super::Edge;

/// 端点相接的容差（逻辑坐标，tick/key 量级远大于此）
const JOIN_EPS: f32 = 1e-2;

/// 把边组装成闭环（渲染层用）：端点首尾相接串联，**保持边原始方向**。
///
/// 输出环点序列首尾重复（闭合）。开放链（找不到相接边的残余边）丢弃；
/// 零长边（重复端点补边）对几何无贡献，直接跳过。
pub(crate) fn assemble_loops(edges: &[Edge]) -> Vec<Vec<(f32, f32)>> {
    let mut unused: Vec<Edge> = edges.to_vec();
    let mut loops: Vec<Vec<(f32, f32)>> = Vec::new();
    'outer: while let Some(first) = unused.pop() {
        if is_zero_len(first) {
            continue;
        }
        let mut pts = vec![first.0, first.1];
        let mut head = first.1;
        loop {
            let mut i = 0;
            loop {
                if i >= unused.len() {
                    continue 'outer; // 开放链：丢弃本环，继续下一环
                }
                let (a, b) = unused[i];
                // 找以 head 为端点的边：起点相接保持方向，终点相接反向串联
                let (next, joined) = if dist_eq(a, head) {
                    (b, true)
                } else if dist_eq(b, head) {
                    (a, true)
                } else {
                    (a, false)
                };
                if !joined {
                    i += 1;
                    continue;
                }
                if is_zero_len(unused[i]) {
                    unused.remove(i);
                    continue;
                }
                unused.remove(i);
                pts.push(next);
                if dist_eq(next, pts[0]) {
                    loops.push(pts);
                    continue 'outer; // 闭合完成
                }
                head = next;
                break;
            }
        }
    }
    loops
}

/// 零长边（首尾同点）
fn is_zero_len(e: Edge) -> bool {
    dist_eq(e.0, e.1)
}

/// 端点距离判定（容差 JOIN_EPS）
fn dist_eq(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < JOIN_EPS && (a.1 - b.1).abs() < JOIN_EPS
}

/// 点是否在环内（绕数 ≠ 0；环点序列首尾重复，windows(2) 覆盖全部边）。
///
/// 与 `winding_number` 同规则（半开区间），供渲染层判定"环是否已填充"。
pub(crate) fn loop_contains_point(loop_pts: &[(f32, f32)], px: f32, py: f32) -> bool {
    let mut wn = 0;
    for w in loop_pts.windows(2) {
        let (ax, ay) = w[0];
        let (bx, by) = w[1];
        let cross = (bx - ax) * (py - ay) - (px - ax) * (by - ay);
        if ay <= py {
            if by > py && cross > 0.0 {
                wn += 1;
            }
        } else if by <= py && cross < 0.0 {
            wn -= 1;
        }
    }
    wn != 0
}

/// 格点是否被环**覆盖**（填充判定）：中心在环内 ∨ 中心距环任一边
/// < 半格（snap 方向按 snap 归一、key 方向按 1 归一）。
///
/// 消除"边缘小空缺"：斜边/弯曲边界恰好穿过格点中心附近时，
/// 中心判定会把这些格点排除（压线归外 + 图形内近线格点可能判外），
/// 视觉上边缘出现单格缺口；半格容差把这些格点归入填充，
/// 使填充边缘 = 图形覆盖的格点（与路径格点/渲染闭环一致）。
pub(crate) fn loop_covers_cell(loop_pts: &[(f32, f32)], cx: f32, cy: f32, snap: f32) -> bool {
    if loop_contains_point(loop_pts, cx, cy) {
        return true;
    }
    let inv_snap = 1.0 / snap.max(1.0);
    for w in loop_pts.windows(2) {
        let (ax, ay) = w[0];
        let (bx, by) = w[1];
        let dx = bx - ax;
        let dy = by - ay;
        let len2 = dx * dx + dy * dy;
        let t = if len2 <= f32::EPSILON {
            0.0
        } else {
            (((cx - ax) * dx + (cy - ay) * dy) / len2).clamp(0.0, 1.0)
        };
        let px = ax + t * dx;
        let py = ay + t * dy;
        let d2 = ((cx - px) * inv_snap).powi(2) + (cy - py).powi(2);
        if d2 < 0.25 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{assemble_loops, loop_contains_point, loop_covers_cell};
    use crate::interaction::line_tool::fill::Edge;

    #[test]
    fn test_assemble_single_rect_loop() {
        // 矩形 4 边（首尾相接、方向不一致）→ 1 个闭合环，首尾重复
        let edges: Vec<Edge> = vec![
            ((0.0, 0.0), (4.0, 0.0)),
            ((4.0, 1.0), (0.0, 1.0)), // 反向的顶边
            ((4.0, 0.0), (4.0, 1.0)),
            ((0.0, 1.0), (0.0, 0.0)),
        ];
        let loops = assemble_loops(&edges);
        assert_eq!(loops.len(), 1, "矩形边组装为一个环");
        let lp = &loops[0];
        assert_eq!(lp.len(), 5, "环点序列首尾重复（4 顶点 + 闭合点）");
        assert_eq!(lp[0], lp[lp.len() - 1], "首尾闭合");
        // 起点取决于组装顺序，断言顶点集合
        let mut verts = lp.clone();
        verts.pop();
        verts.sort_by(|a, b| a.partial_cmp(b).expect("顶点坐标比较不应为 NaN"));
        assert_eq!(
            verts,
            vec![(0.0, 0.0), (0.0, 1.0), (4.0, 0.0), (4.0, 1.0)],
            "环包含矩形全部 4 个顶点"
        );
    }

    #[test]
    fn test_assemble_two_independent_loops() {
        // 两个分离矩形 → 2 个环
        let edges: Vec<Edge> = vec![
            ((0.0, 0.0), (2.0, 0.0)),
            ((2.0, 0.0), (2.0, 2.0)),
            ((2.0, 2.0), (0.0, 2.0)),
            ((0.0, 2.0), (0.0, 0.0)),
            ((10.0, 0.0), (12.0, 0.0)),
            ((12.0, 0.0), (12.0, 2.0)),
            ((12.0, 2.0), (10.0, 2.0)),
            ((10.0, 2.0), (10.0, 0.0)),
        ];
        assert_eq!(assemble_loops(&edges).len(), 2, "两个独立图形各自成环");
    }

    #[test]
    fn test_assemble_open_chain_dropped() {
        // 开放链（端点无配对）→ 不成环
        let edges: Vec<Edge> = vec![((0.0, 0.0), (4.0, 0.0)), ((4.0, 0.0), (8.0, 0.0))];
        assert!(assemble_loops(&edges).is_empty(), "开放链不成环");
    }

    #[test]
    fn test_assemble_skips_zero_len_edge() {
        // 重复端点补边（零长边）混入 → 不影响成环
        let edges: Vec<Edge> = vec![
            ((0.0, 0.0), (4.0, 0.0)),
            ((4.0, 0.0), (4.0, 1.0)),
            ((4.0, 1.0), (0.0, 1.0)),
            ((0.0, 1.0), (0.0, 0.0)),
            ((4.0, 0.0), (4.0, 0.0)), // 零长边
        ];
        assert_eq!(assemble_loops(&edges).len(), 1, "零长边被跳过");
    }

    #[test]
    fn test_loop_contains_point_in_and_out() {
        let lp = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)];
        assert!(loop_contains_point(&lp, 2.0, 2.0), "环内点");
        assert!(!loop_contains_point(&lp, 5.0, 2.0), "环外点");
        assert!(!loop_contains_point(&lp, 2.0, -1.0), "环上方外点");
    }

    /// 斜边经过格点中心（压线）：中心判定排除，覆盖判定归入填充
    #[test]
    fn test_loop_covers_cell_edge_pressure() {
        // 斜边 (4,0)→(0,4)（y = 4-x）：格点中心 (2,2) 恰在边上
        let _lp = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)];
        // 斜边 = (4,0)→(4,4)？不，用五边形场景：斜边 (2,2)→(0,4)？简化：
        // 三角形 (0,0)-(4,0)-(0,4)：斜边 (4,0)→(0,4)，中心 (2,2) 压线
        let tri = vec![(0.0, 0.0), (4.0, 0.0), (0.0, 4.0), (0.0, 0.0)];
        // 压线点 (2,2)：中心判定 = false（线上，绕数 0 边界歧义）
        assert!(!loop_contains_point(&tri, 2.0, 2.0), "压线点中心判定为外");
        assert!(
            loop_covers_cell(&tri, 2.0, 2.0, 1.0),
            "压线格点被覆盖判定归入填充"
        );
        // 距边 0.4 格的环外点 → 覆盖（半格容差内）
        assert!(loop_covers_cell(&tri, 2.4, 0.4, 1.0), "距边 0.4 格在容差内");
        // 距斜边超过半格的点 → 不覆盖（(4.5,0.5) 到斜边 ≈ 0.71）
        assert!(!loop_covers_cell(&tri, 4.5, 0.5, 1.0), "距边超过半格不覆盖");
        // 内部格点中心 → 覆盖（中心判定分支）
        assert!(loop_covers_cell(&tri, 0.6, 0.6, 1.0), "内部格点覆盖");
        let lp = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)];
        assert!(
            loop_covers_cell(&lp, 2.0, 2.0, 1.0) && !loop_covers_cell(&lp, 4.5, 2.0, 1.0),
            "矩形：内部覆盖、远处不覆盖"
        );
    }
}
