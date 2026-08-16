//! 颜料桶填充：按**基本矢量绘制软件**模式工作
//!
//! - **点击时**（`handle_fill_pressed`）：只在 `LineToolState.fill` 记录
//!   一个**标记**（点击格点），不做任何几何计算 → 点击永不失败；
//! - **√ 确认时**（`confirm_fill_cells`）：根据全部路径的封闭图形覆盖
//!   范围计算该区域覆盖的全部格点，与路径格点合并生成实心音符。
//!
//! 区域判定 = 格点中心 vs 闭环绕数（环包含集合），与渲染层
//! （`fill_region`/`build_fill_path`）同规则 → 音符覆盖与填充显示
//! 永远一致（边框贴合封闭图形）。未封闭路径填充蔓延到视图可见范围
//! 边界（与绘图软件"填充到画布边缘"行为一致），Ctrl+Z 可撤销。

use super::geom;
use crate::Editor;
use iced_core::Point;
use loops::{assemble_loops, loop_covers_cell};
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
pub(crate) fn collect_edges(paths: &[LinePath], snap: f32) -> Vec<Edge> {
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

/// 格点的区域身份：被哪些闭环**覆盖**（位 i = 被第 i 个环覆盖）。
/// 区域 = 环覆盖集合；背景区域 = 0。与渲染层 `loop_covers_cell`
/// （中心在环内 ∨ 距边 < 半格）同规则 → 语义与视觉永远一致，
/// 且消除斜边/弯曲边界处的边缘格点空缺。
fn region_key(loops: &[Vec<(f32, f32)>], cx: f32, cy: f32, snap: f32) -> u128 {
    let mut key = 0u128;
    for (i, lp) in loops.iter().enumerate().take(128) {
        if loop_covers_cell(lp, cx, cy, snap) {
            key |= 1u128 << i;
        }
    }
    key
}

/// √ 确认时按**图形覆盖范围**计算填充音符格点（纯函数）：
///
/// 1. 全部边组装成闭环（`assemble_loops`；开放链不成环 → 全部区域
///    身份 = 背景，蔓延行为不变）；
/// 2. 每个填充标记确定一个区域（环覆盖集合），返回该区域在范围内
///    的全部格点；多个标记 = 区域并集（去重）。
///
/// - 标记在封闭曲线内部 → 该轮廓内部全部格点；
/// - 标记在外部（环外）→ 背景 = 全部环外格点（蔓延到范围边界）；
/// - 多个独立图形：每个标记只覆盖其所在区域；
/// - 嵌套图形：内层环内标记身份 = {外, 内}，与外层（{外}）区分，洞保留。
///
/// 判定 = 格点**中心** vs 闭环绕数（snap 网格是唯一精度），与渲染层
/// 同规则 → 生成的音符覆盖 = 填充显示区域，边框一致。
pub fn confirm_fill_cells(
    edges: &[Edge],
    snap: f32,
    marks: &[(f32, u16)],
    tick_idx_range: (i64, i64),
    key_range: (u16, u16),
) -> Vec<(i64, u16)> {
    if marks.is_empty() || edges.is_empty() {
        return Vec::new();
    }
    let loops = assemble_loops(edges);
    // 标记区域身份集合（去重：同一区域多次点击只算一次）
    let mut regions = HashSet::new();
    for &(t, k) in marks {
        regions.insert(region_key(&loops, t + snap * 0.5, k as f32 + 0.5, snap));
    }
    let mut cells = Vec::new();
    for ti in tick_idx_range.0..=tick_idx_range.1 {
        for k in key_range.0..=key_range.1 {
            let c = ((ti as f32 + 0.5) * snap, k as f32 + 0.5);
            if regions.contains(&region_key(&loops, c.0, c.1, snap)) {
                cells.push((ti, k));
            }
        }
    }
    cells
}

impl Editor {
    /// 颜料桶点击：记录一个**填充标记**（不计算格点、不生成音符）。
    ///
    /// 与基本矢量绘制软件一致：点击只标记区域，√ 确认时再按图形
    /// 覆盖范围计算音符（`confirm_fill_cells`）。
    ///
    /// - 新点击：标记追加进 `fill`（去重），记录一次路径历史（Ctrl+Z 可撤销）；
    /// - 点击已标记格点：清除**全部**标记（再点一次取消）；
    /// - 无完整路径时忽略（封闭区域不存在）。
    ///
    /// `pub(crate)`：pressed.rs（interaction 父模块）在 Curve 工具 + 填充
    /// 模式下调用。
    pub(crate) fn handle_fill_pressed(&mut self, _pos: Point, snapped_tick: f32, key: u16) {
        // 无完整路径 → 封闭区域不存在，忽略点击
        if !self.editor_state.line_tool.is_complete() {
            tracing::debug!("颜料桶: 无完整路径，未填充");
            return;
        }
        // 点击已标记格点 → 取消全部填充；否则记录标记。均记录历史。
        let line = &mut self.editor_state.line_tool;
        let click_on_fill = line.fill.contains(&(snapped_tick, key));
        let changed = if click_on_fill {
            line.clear_fill()
        } else {
            line.add_fill_marks(&[(snapped_tick, key)]) > 0
        };
        if !changed {
            return;
        }
        line.push_path_history();
        line.last_push_path = None;
        tracing::info!(
            "颜料桶: 标记 {} 个区域（累计 {}），√ 确认时按覆盖范围生成音符",
            line.fill.len(),
            line.fill.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Edge, confirm_fill_cells};

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
        let mut cells = confirm_fill_cells(&rect_edges(), 1.0, &[(3.0, 3)], (0, 100), (0, 20));
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
        let cells = confirm_fill_cells(&edges, 1.0, &[(15.0, 8)], (0, 1000), (0, 100));
        assert_eq!(cells.len(), 9 * 7, "中心在轮廓内的格点全部铺满");
    }

    #[test]
    fn test_fill_click_on_boundary_cell_fills_side() {
        // 标记边界格点 (2,2)：其中心 (2.5,2.5) 在矩形内 → 填内部
        let mut cells = confirm_fill_cells(&rect_edges(), 1.0, &[(2.0, 2)], (0, 100), (0, 20));
        cells.sort();
        assert_eq!(cells.len(), 6, "边界格点归属其中心所在侧");
        assert!(cells.contains(&(3, 3)), "内部格点可达");
    }

    #[test]
    fn test_fill_outside_rect_spreads_to_bounds() {
        // 标记 (0,0) 在矩形外 → 背景蔓延整个范围，矩形内部 6 格被隔离
        let cells = confirm_fill_cells(&rect_edges(), 1.0, &[(0.0, 0)], (-5, 5), (0, 5));
        // 范围 11x6 = 66 格 - 内部 6 格 = 60
        assert_eq!(cells.len(), 60, "外部连通区 = 全部 - 被隔离内部");
        assert!(!cells.contains(&(3, 3)), "矩形内部格点不可达");
        assert!(!cells.contains(&(3, 4)), "矩形内部格点不可达");
    }

    #[test]
    fn test_fill_honors_tick_bounds() {
        // 标记 (3,3) 内部格点；tick 范围 [3,3]、key 范围 [3,3] → 只填起点
        let cells = confirm_fill_cells(&rect_edges(), 1.0, &[(3.0, 3)], (3, 3), (3, 3));
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
        // 同一闭环内：腰部宽 0.8，格点中心无法落入（旧中心判定隔断
        // → 音符空缺）；半格容差让腰部两侧格点（距腰边 0.1）被覆盖
        // → 上下两瓣全部填充（回归：网格泛洪曾造成音符空缺）
        let edges = waist_edges();
        let cells = confirm_fill_cells(&edges, 1.0, &[(4.0, 0)], (0, 7), (0, 7));
        assert_eq!(
            cells.len(),
            52,
            "上 3 行 + 下 3 行 × 8 列 + 腰部 4 格全部填充"
        );
        assert!(cells.contains(&(7, 7)), "腰部另一侧最远角点可达");
        assert!(cells.contains(&(7, 0)), "起点侧最远角点可达");
    }

    #[test]
    fn test_fill_background_waist_region() {
        // 标记腰部环外格点 → 背景 = 全部环外（范围 64 − 覆盖 52 = 12）
        let edges = waist_edges();
        let cells = confirm_fill_cells(&edges, 1.0, &[(5.0, 4)], (0, 7), (0, 7));
        assert_eq!(cells.len(), 12, "背景 = 全部环外格点");
        assert!(!cells.contains(&(4, 0)), "环内格点不可达");
    }

    #[test]
    fn test_fill_two_loops_click_inside_fills_only_that_loop() {
        // 两个独立矩形环：标记 A 内 → √ 时只填 A（B 区域身份不同）
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
        let cells = confirm_fill_cells(&edges, 1.0, &[(2.0, 2)], (0, 15), (0, 5));
        assert_eq!(cells.len(), 16, "标记 A 内只填 A（4x4=16）");
        assert!(cells.iter().all(|&(ti, _)| ti <= 3), "不跨到 B");
        // 标记背景 → 全部环外（范围 16x6=96 − 两环 32 = 64）
        let cells = confirm_fill_cells(&edges, 1.0, &[(7.0, 2)], (0, 15), (0, 5));
        assert_eq!(cells.len(), 64, "背景 = 全部环外格点");
        assert!(!cells.contains(&(2, 2)), "A 内格点不可达");
        assert!(!cells.contains(&(12, 2)), "B 内格点不可达");
        // 两个标记（A 内 + B 内）→ 两个区域并集
        let cells = confirm_fill_cells(&edges, 1.0, &[(2.0, 2), (12.0, 2)], (0, 15), (0, 5));
        assert_eq!(cells.len(), 32, "多标记 = 区域并集");
    }

    #[test]
    fn test_fill_nested_loop_keeps_hole() {
        // 外环 (0,0)-(8,8) + 内环 (3,3)-(5,5)：
        // 标记外层内部 → 外层除内层（洞保留）；标记内层 → 只填内层
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
        let cells = confirm_fill_cells(&edges, 1.0, &[(1.0, 1)], (0, 7), (0, 7));
        assert_eq!(cells.len(), 60, "外层填充保留内层洞（64-4）");
        assert!(!cells.contains(&(4, 4)), "洞内格点不填");
        let cells = confirm_fill_cells(&edges, 1.0, &[(4.0, 4)], (0, 7), (0, 7));
        assert_eq!(cells.len(), 4, "标记内层 → 只填内层");
    }

    /// 五边形斜边 (1920,62)→(960,64) 恰好经过格点 (960,63) 中心 (1200,63.5)：
    /// 旧中心判定压线格点判外 → 边缘单格空缺；半格容差覆盖修复。
    #[test]
    fn test_fill_slanted_edge_no_gaps() {
        let edges: Vec<Edge> = vec![
            ((0.0, 60.0), (1920.0, 60.0)),
            ((1920.0, 60.0), (1920.0, 62.0)),
            ((1920.0, 62.0), (960.0, 64.0)),
            ((960.0, 64.0), (0.0, 62.0)),
            ((0.0, 62.0), (0.0, 60.0)),
        ];
        let cells = confirm_fill_cells(&edges, 480.0, &[(480.0, 61)], (0, 10), (0, 127));
        // tick=960 列（中心 x=1200，斜边 y=63.5）：压线格点 (960,63) 必须覆盖
        assert!(
            cells.contains(&(2, 63)),
            "斜边压线格点 (960,63) 覆盖（旧判定缺失）"
        );
        // tick=1440 列（中心 x=1680，斜边 y=62.5）：近线格点 (1440,62) 必须覆盖
        assert!(
            cells.contains(&(3, 62)),
            "斜边近线格点 (1440,62) 覆盖（旧判定缺失）"
        );
        // 内部格点正常
        assert!(cells.contains(&(2, 60)), "内部格点 (960,60) 覆盖");
        assert!(cells.contains(&(0, 62)), "左边缘内部格点 (0,62) 覆盖");
    }
}
