//! 颜料桶填充：几何泛洪填充曲线围成的封闭区域为**实心**
//!
//! ## 算法（矢量颜料桶）
//! 1. 收集全部路径段的贝塞尔折线（自动柄 = 直线段；自定义柄 = 16 段
//!    折线逼近）作为**几何边界**；
//! 2. 点击格点中心做绕数判定（winding number，半开区间）：
//!    - 在封闭曲线内部（绕数 ≠ 0）→ 填充该轮廓内部；
//!    - 在外部（绕数 = 0）→ 背景蔓延填充（到画布可见范围边界）；
//! 3. 范围内每个格点中心做绕数测试，与点击点绕数相同的格点构成候选集，
//!    再 BFS 连通剪枝（只填与点击点连通的同绕数区域，不跨图形边界）；
//! 4. 结果存入曲线工具编辑层（`LineToolState.fill`），√ 确认时与路径
//!    一起生成实心音符（**不直接写入音符**）。
//!
//! 几何判定不依赖栅格化：曲线采样跳格/缝隙不会导致漏穿，填充由曲线
//! 几何决定（"完全铺满封闭轮廓内部"），与网格泛洪无关。未封闭路径
//! 填充会蔓延到视图可见范围边界（与绘图软件"填充到画布边缘"行为
//! 一致），Ctrl+Z 可撤销。

use super::geom;
use crate::Editor;
use iced_core::Point;
use lumino_editor_state::LinePath;
use std::collections::{HashSet, VecDeque};

/// 渲染层闭环组装/环内判定（assemble_loops、loop_contains_point）
pub(crate) mod loops;
/// 渲染层填充区域几何（fill_region）
pub(crate) mod region;

/// 折线边（逻辑坐标 (tick, key) 端对）
pub(crate) type Edge = ((f32, f32), (f32, f32));

/// 贝塞尔段折线逼近采样数（与 `point_curve_distance` 一致）
const CURVE_SEGMENTS: usize = 16;

/// 全部路径段的几何折线：自动柄段（未弯曲）= 直线直接用两端点；
/// 自定义柄段 = 16 段折线逼近。
///
/// 另做**端点容差闭合**：任意两条路径（或同路径）的端点距离 ≤ 1 格
/// （tick ≤ snap、key ≤ 1）时补一条隐式连接边——矢量编辑器填充对
/// 未精确闭合的轮廓自动封口，手画封闭图形（接缝差一两格）也能填。
fn collect_edges(paths: &[LinePath], snap: f32) -> Vec<Edge> {
    let mut edges = Vec::new();
    for path in paths {
        if path.len() < 2 {
            continue;
        }
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.handles_auto && b.handles_auto {
                edges.push((a.pos, b.pos));
                continue;
            }
            let cp1 = a.out_handle_abs();
            let cp2 = b.in_handle_abs();
            let mut prev = a.pos;
            for i in 1..=CURVE_SEGMENTS {
                let t = i as f32 / CURVE_SEGMENTS as f32;
                let cur = geom::bezier_point(a.pos, cp1, cp2, b.pos, t);
                edges.push((prev, cur));
                prev = cur;
            }
        }
    }
    // 端点容差闭合（含跨路径接缝；零长/重复边对绕数无贡献，无害）
    let mut endpoints: Vec<(f32, f32)> = Vec::new();
    for path in paths {
        if path.len() >= 2 {
            endpoints.push(path[0].pos);
            endpoints.push(path[path.len() - 1].pos);
        }
    }
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    for i in 0..endpoints.len() {
        for j in (i + 1)..endpoints.len() {
            let (a, b) = (endpoints[i], endpoints[j]);
            if (a.0 - b.0).abs() <= snap && (a.1 - b.1).abs() <= 1.0 && seen.insert((i, j)) {
                edges.push((a, b));
            }
        }
    }
    edges
}

/// 绕数（winding number，半开区间规则）：点在边**上**时不翻转——
/// 浮点边界歧义归入"下方"区域，保证闭区间一致性。
fn winding_number(px: f32, py: f32, edges: &[Edge]) -> i32 {
    let mut wn = 0;
    for &((ax, ay), (bx, by)) in edges {
        let cross = (bx - ax) * (py - ay) - (px - ax) * (by - ay);
        if ay <= py {
            if by > py && cross > 0.0 {
                wn += 1;
            }
        } else if by <= py && cross < 0.0 {
            wn -= 1;
        }
    }
    wn
}

/// 几何泛洪填充纯函数（矢量颜料桶核心）：
///
/// 1. 点击格点中心绕数 `w0`；候选集 = 范围内**所有**绕数与 `w0` 相同的格点
///    （几何内部判定——曲线缝隙/采样跳格不影响结果，完全铺满轮廓内部）；
/// 2. BFS 从点击格点出发只走候选集 → 与点击点连通的同绕数区域
///    （不跨曲线边界、不跨其他图形的内部）。
///
/// - 点击封闭曲线内部（w0 ≠ 0）→ 该轮廓内部全部格点；
/// - 点击外部（w0 = 0）→ 背景蔓延到范围边界（遇轮廓停止）。
///
/// 格点归属判定 = 格点**中心**是否在轮廓内（snap 网格是唯一精度）。
pub fn fill_cells(
    edges: &[Edge],
    snap: f32,
    start: (i64, u16),
    tick_idx_range: (i64, i64),
    key_range: (u16, u16),
) -> Vec<(i64, u16)> {
    let (si, sk) = start;
    let s_center = ((si as f32 + 0.5) * snap, sk as f32 + 0.5);
    let w0 = winding_number(s_center.0, s_center.1, edges);
    // 1. 候选集：与点击点同绕数的全部格点
    let mut candidates = HashSet::new();
    for ti in tick_idx_range.0..=tick_idx_range.1 {
        for k in key_range.0..=key_range.1 {
            let c = ((ti as f32 + 0.5) * snap, k as f32 + 0.5);
            if winding_number(c.0, c.1, edges) == w0 {
                candidates.insert((ti, k));
            }
        }
    }
    // 2. BFS 连通剪枝：候选集外的格点不可走（曲线边界天然隔断）
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some((ti, k)) = queue.pop_front() {
        for (dti, dk) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nti = ti + dti;
            let nk = k as i64 + dk;
            if nti < tick_idx_range.0
                || nti > tick_idx_range.1
                || nk < key_range.0 as i64
                || nk > key_range.1 as i64
            {
                continue;
            }
            let next = (nti, nk as u16);
            if !candidates.contains(&next) {
                continue;
            }
            if visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    visited.into_iter().collect()
}

impl Editor {
    /// 颜料桶填充处理：点击曲线区域 → 几何泛洪计算格点，
    /// **存入曲线工具编辑层**（`line_tool.fill`），不直接生成音符。
    ///
    /// - 新填充：格点合并进 `fill`（去重），记录一次路径历史（Ctrl+Z 可撤销）；
    /// - 点击已填充格点：清除**全部**填充（再点一次取消，也记录历史）；
    /// - √ 确认时 `confirm_line_tool` 将路径格点 + 填充格点合并生成实心音符；
    /// - × 清空时一并丢弃。
    ///
    /// 边界 = 全部路径段几何折线；范围 = 画布可见 tick 区间 + 全键盘 key。
    /// 填充模式保持开启（开关式，可连续填充多个区域）。
    ///
    /// `pub(crate)`：pressed.rs（interaction 父模块）在 Curve 工具 + 填充
    /// 模式下调用。
    pub(crate) fn handle_fill_pressed(&mut self, _pos: Point, snapped_tick: f32, key: u16) {
        let snap = self.editor_state.view.snap_precision.max(1.0);
        // 1. 几何边界（全部路径段的贝塞尔折线 + 端点容差闭合）
        let edges = collect_edges(&self.editor_state.line_tool.paths, snap);
        if edges.is_empty() {
            tracing::debug!("颜料桶: 无完整路径，未填充");
            return;
        }
        // 2. 可见范围（tick 方向 = 画布可见区间；key 方向 = 全键盘）
        let tick_lo = self.x_to_tick(0.0).max(0.0);
        let tick_hi = self
            .x_to_tick(self.editor_state.canvas.size_x)
            .max(tick_lo + snap);
        let key_count = self.editor_state.view.key_count;
        let start = ((snapped_tick / snap).round() as i64, key);

        // 3. 几何泛洪 → 逻辑坐标格点
        let cells = fill_cells(
            &edges,
            snap,
            start,
            (
                (tick_lo / snap).floor() as i64,
                (tick_hi / snap).ceil() as i64,
            ),
            (0, key_count.saturating_sub(1)),
        );
        let cells: Vec<(f32, u16)> = cells
            .into_iter()
            .map(|(ti, k)| (ti as f32 * snap, k))
            .collect();
        if cells.is_empty() {
            tracing::debug!("颜料桶: 点击位置无可用格点，未填充");
            return;
        }

        // 4. 点击已填充格点 → 取消全部填充；否则合并新格点。均记录历史。
        let line = &mut self.editor_state.line_tool;
        let click_on_fill = line.fill.contains(&(snapped_tick, key));
        let changed = if click_on_fill {
            line.clear_fill()
        } else {
            line.add_fill_cells(&cells) > 0
        };
        if !changed {
            return;
        }
        line.push_path_history();
        line.last_push_path = None;
        tracing::info!(
            "颜料桶: {} 个格点（累计 {}），累计填充 {} 格",
            cells.len(),
            line.fill.len(),
            line.fill.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Edge, fill_cells};

    /// 矩形四条边折线（snap=1 时逻辑坐标 == 格索引）：(2,2)-(4,5)
    fn rect_edges() -> Vec<Edge> {
        vec![
            ((2.0, 2.0), (4.0, 2.0)),
            ((4.0, 2.0), (4.0, 5.0)),
            ((4.0, 5.0), (2.0, 5.0)),
            ((2.0, 5.0), (2.0, 2.0)),
        ]
    }

    #[test]
    fn test_fill_inside_rect() {
        // 矩形 (2,2)-(4,5)：格点中心 ∈ (2,4)×(2,5) → 格 (2..3, 2..4) = 6 格
        let mut cells = fill_cells(&rect_edges(), 1.0, (3, 3), (0, 100), (0, 20));
        cells.sort();
        assert_eq!(
            cells,
            vec![(2, 2), (2, 3), (2, 4), (3, 2), (3, 3), (3, 4)],
            "轮廓内部格点完全铺满（含边界上中心在内的格子）"
        );
    }

    #[test]
    fn test_fill_larger_enclosed_region() {
        // 10x8 矩形（x: 10..=19, y: 5..=12）→ 中心在内格点 = 9x7 = 63 格
        let edges = vec![
            ((10.0, 5.0), (19.0, 5.0)),
            ((19.0, 5.0), (19.0, 12.0)),
            ((19.0, 12.0), (10.0, 12.0)),
            ((10.0, 12.0), (10.0, 5.0)),
        ];
        let cells = fill_cells(&edges, 1.0, (15, 8), (0, 1000), (0, 100));
        assert_eq!(cells.len(), 9 * 7, "中心在轮廓内的格点全部铺满");
    }

    #[test]
    fn test_fill_click_on_boundary_cell_fills_side() {
        // 点击边界格点 (2,2)：其中心 (2.5,2.5) 在矩形内 → 填内部
        let mut cells = fill_cells(&rect_edges(), 1.0, (2, 2), (0, 100), (0, 20));
        cells.sort();
        assert_eq!(cells.len(), 6, "边界格点归属其中心所在侧");
        assert!(cells.contains(&(3, 3)), "内部格点可达");
    }

    #[test]
    fn test_fill_outside_rect_spreads_to_bounds() {
        // 起点 (0,0) 在矩形外 → 背景蔓延整个范围，矩形内部 6 格被隔离
        let cells = fill_cells(&rect_edges(), 1.0, (0, 0), (-5, 5), (0, 5));
        // 范围 11x6 = 66 格 - 内部 6 格 = 60
        assert_eq!(cells.len(), 60, "外部连通区 = 全部 - 被隔离内部");
        assert!(!cells.contains(&(3, 3)), "矩形内部格点不可达");
        assert!(!cells.contains(&(3, 4)), "矩形内部格点不可达");
    }

    #[test]
    fn test_fill_honors_tick_bounds() {
        // 起点 (3,3) 内部格点；tick 范围 [3,3]、key 范围 [3,3] →
        // 邻居 (3,4) 超 key 界被裁剪，只填起点
        let cells = fill_cells(&rect_edges(), 1.0, (3, 3), (3, 3), (3, 3));
        assert_eq!(cells, vec![(3, 3)], "超界邻居被裁剪");
    }
}
