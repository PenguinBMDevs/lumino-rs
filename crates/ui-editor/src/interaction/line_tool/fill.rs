//! 颜料桶填充：几何填充曲线围成的封闭区域为**实心**
//!
//! ## 算法（矢量颜料桶）
//! 1. 收集全部路径段的贝塞尔折线（自动柄 = 直线段；自定义柄 = 16 段
//!    折线逼近）作为**几何边界**，端点容差闭合后组装成闭环；
//! 2. 区域身份 = 格点中心的"环包含集合"（被哪些闭环包含）。点击格点
//!    的身份决定填充区域：
//!    - 在封闭曲线内部 → 填充该轮廓内部全部格点；
//!    - 在外部（环外）→ 背景蔓延填充（到画布可见范围边界）；
//! 3. 结果存入曲线工具编辑层（`LineToolState.fill`），√ 确认时与路径
//!    一起生成实心音符（**不直接写入音符**）。
//!
//! 填充判定是纯几何的（格点中心 vs 闭环绕数），**不依赖网格连通性**：
//! 曲线采样跳格/边界格点裂缝/窄通道不会造成区域空缺——视觉（矢量
//! 填充渲染）与音符（格点）始终一致。未封闭路径填充会蔓延到视图
//! 可见范围边界（与绘图软件"填充到画布边缘"行为一致），Ctrl+Z 可撤销。

use super::geom;
use crate::Editor;
use iced_core::Point;
use loops::{assemble_loops, loop_contains_point};
use lumino_editor_state::LinePath;
use std::collections::HashSet;

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

/// 格点的区域身份：被哪些闭环包含（位 i = 被第 i 个环包含）。
/// 区域 = 环包含集合；背景区域 = 0。与渲染层 `loop_contains_point`
/// （绕数 ≠ 0）同规则 → 语义与视觉永远一致。
fn region_key(loops: &[Vec<(f32, f32)>], px: f32, py: f32) -> u128 {
    let mut key = 0u128;
    for (i, lp) in loops.iter().enumerate().take(128) {
        if loop_contains_point(lp, px, py) {
            key |= 1u128 << i;
        }
    }
    key
}

/// 几何填充纯函数（矢量颜料桶核心）：
///
/// 1. 全部边组装成闭环（`assemble_loops`；开放链不成环 → 全部区域
///    身份 = 背景，蔓延行为不变）；
/// 2. 候选 = 与点击格点**同一区域身份**（环包含集合）的全部格点——
///    纯几何判定，不依赖网格连通性：边界格点裂缝/窄通道不会造成
///    区域空缺（"完全铺满封闭轮廓内部"）。
///
/// - 点击封闭曲线内部 → 该轮廓内部全部格点；
/// - 点击外部 → 背景（全部环外格点）蔓延到范围边界；
/// - 多个独立图形：点击其中一个只填它所在环（区域身份不同）；
/// - 嵌套图形：内层环内点身份 = {外, 内}，与外层（{外}）区分，洞保留。
///
/// 格点归属判定 = 格点**中心**是否在环内（snap 网格是唯一精度）。
pub fn fill_cells(
    edges: &[Edge],
    snap: f32,
    start: (i64, u16),
    tick_idx_range: (i64, i64),
    key_range: (u16, u16),
) -> Vec<(i64, u16)> {
    let loops = assemble_loops(edges);
    let (si, sk) = start;
    let s_center = ((si as f32 + 0.5) * snap, sk as f32 + 0.5);
    let s_region = region_key(&loops, s_center.0, s_center.1);
    let mut cells = Vec::new();
    for ti in tick_idx_range.0..=tick_idx_range.1 {
        for k in key_range.0..=key_range.1 {
            let c = ((ti as f32 + 0.5) * snap, k as f32 + 0.5);
            if region_key(&loops, c.0, c.1) == s_region {
                cells.push((ti, k));
            }
        }
    }
    cells
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

    /// 沙漏形环：上/下两个全宽矩形由宽 0.8 的窄腰连接（x ∈ (3.6, 4.4)），
    /// 腰部没有格点中心落入（且不压边界线）→ 4-连通被完全隔断。
    fn waist_edges() -> Vec<Edge> {
        vec![
            ((0.0, 0.0), (8.0, 0.0)),
            ((8.0, 0.0), (8.0, 3.0)),
            ((8.0, 3.0), (4.4, 3.0)),
            ((4.4, 3.0), (4.4, 5.0)),
            ((4.4, 5.0), (8.0, 5.0)),
            ((8.0, 5.0), (8.0, 8.0)),
            ((8.0, 8.0), (0.0, 8.0)),
            ((0.0, 8.0), (0.0, 5.0)),
            ((0.0, 5.0), (3.6, 5.0)),
            ((3.6, 5.0), (3.6, 3.0)),
            ((3.6, 3.0), (0.0, 3.0)),
            ((0.0, 3.0), (0.0, 0.0)),
        ]
    }

    #[test]
    fn test_fill_no_gap_across_narrow_waist() {
        // 同一闭环内：腰部隔断 4-连通，但上下两瓣区域身份相同（同一环）
        // → 必须全部填充（回归：网格泛洪在此处造成音符空缺）
        let edges = waist_edges();
        let cells = fill_cells(&edges, 1.0, (4, 0), (0, 7), (0, 7));
        assert_eq!(cells.len(), 48, "上 3 行 + 下 3 行 × 8 列全部填充");
        assert!(cells.contains(&(7, 7)), "腰部另一侧最远角点可达");
        assert!(cells.contains(&(7, 0)), "起点侧最远角点可达");
    }

    #[test]
    fn test_fill_background_waist_region() {
        // 点击腰部环外格点 → 背景 = 全部环外（腰部 2 行 × 8 列 = 16）
        let edges = waist_edges();
        let cells = fill_cells(&edges, 1.0, (5, 4), (0, 7), (0, 7));
        assert_eq!(cells.len(), 16, "背景 = 全部环外格点");
        assert!(!cells.contains(&(4, 0)), "环内格点不可达");
    }

    #[test]
    fn test_fill_two_loops_click_inside_fills_only_that_loop() {
        // 两个独立矩形环：点击 A 内 → 只填 A（B 区域身份不同）
        let edges: Vec<Edge> = vec![
            ((0.0, 0.0), (4.0, 0.0)),
            ((4.0, 0.0), (4.0, 4.0)),
            ((4.0, 4.0), (0.0, 4.0)),
            ((0.0, 4.0), (0.0, 0.0)),
            ((10.0, 0.0), (14.0, 0.0)),
            ((14.0, 0.0), (14.0, 4.0)),
            ((14.0, 4.0), (10.0, 4.0)),
            ((10.0, 4.0), (10.0, 0.0)),
        ];
        let cells = fill_cells(&edges, 1.0, (2, 2), (0, 15), (0, 5));
        assert_eq!(cells.len(), 16, "点击 A 内只填 A（4x4=16）");
        assert!(cells.iter().all(|&(ti, _)| ti <= 3), "不跨到 B");
        // 点击背景 → 全部环外（范围 16x6=96 − 两环 32 = 64）
        let cells = fill_cells(&edges, 1.0, (7, 2), (0, 15), (0, 5));
        assert_eq!(cells.len(), 64, "背景 = 全部环外格点");
        assert!(!cells.contains(&(2, 2)), "A 内格点不可达");
        assert!(!cells.contains(&(12, 2)), "B 内格点不可达");
    }

    #[test]
    fn test_fill_nested_loop_keeps_hole() {
        // 外环 (0,0)-(8,8) + 内环 (3,3)-(5,5)：
        // 点击外层内部 → 外层除内层（洞保留）；点击内层 → 只填内层
        let edges: Vec<Edge> = vec![
            ((0.0, 0.0), (8.0, 0.0)),
            ((8.0, 0.0), (8.0, 8.0)),
            ((8.0, 8.0), (0.0, 8.0)),
            ((0.0, 8.0), (0.0, 0.0)),
            ((3.0, 3.0), (5.0, 3.0)),
            ((5.0, 3.0), (5.0, 5.0)),
            ((5.0, 5.0), (3.0, 5.0)),
            ((3.0, 5.0), (3.0, 3.0)),
        ];
        let cells = fill_cells(&edges, 1.0, (1, 1), (0, 7), (0, 7));
        assert_eq!(cells.len(), 60, "外层填充保留内层洞（64-4）");
        assert!(!cells.contains(&(4, 4)), "洞内格点不填");
        let cells = fill_cells(&edges, 1.0, (4, 4), (0, 7), (0, 7));
        assert_eq!(cells.len(), 4, "点击内层 → 只填内层");
    }
}
