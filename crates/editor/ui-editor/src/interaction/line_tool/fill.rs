//! 颜料桶填充：按**基本矢量绘制软件**模式工作
//!
//! - **点击时**（`handle_fill_pressed`）：只在 `LineToolState.fill` 记录
//!   一个**标记**（点击格点），不做任何几何计算 → 点击永不失败；
//! - **√ 确认时**（[`spans::fill_spans`]）：把全部路径组装成闭环，对每条音高行
//!   解析求出封闭图形内部与该行相交的**时间区间**，每个区间一条音符。
//!
//! 区域判定 = 闭环覆盖集合（环包含集合），与渲染层
//! （`fill_region`/`build_fill_path`）同规则 → 音符覆盖与填充显示一致
//! （边框贴合封闭图形）。未封闭路径填充蔓延到视图可见范围边界
//! （与绘图软件"填充到画布边缘"行为一致），Ctrl+Z 可撤销。
//!
//! 音符边界**不做任何精度量化**：区间端点就是闭环边与音高行边界的解析交点
//! （见 [`spans`]）。`snap` 只用于两件事——还原**输入位置**（标记所在格的中心）
//! 与判定两条笔画的端点是否算「接上」（与 Spiderweb 的固定咬合容差同义）。

use super::geom;
use crate::Editor;
use iced_core::Point;
use lumino_editor_state::LinePath;
use std::collections::HashSet;

/// 渲染层闭环组装（assemble_loops、loop_contains_point）
pub(crate) mod loops;
/// 渲染层填充区域几何（fill_region）
pub(crate) mod region;
/// 逐音高行填充区间（闭包内部 → 音符）
pub(crate) mod spans;

pub(crate) use loops::assemble_loops;
pub(crate) use spans::{fill_spans, mark_regions};

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
///
/// `snap` 在这里是**输入侧咬合容差**（两条笔画算不算接上），不参与
/// 音符边界量化 —— 与 Spiderweb 的 `TOUCH_BEATS` / `TOUCH_KEYS` 同义。
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

impl Editor {
    /// 颜料桶点击：记录一个**填充标记**（不计算格点、不生成音符）。
    ///
    /// 与基本矢量绘制软件一致：点击只标记区域，√ 确认时再按图形
    /// 覆盖范围计算音符（[`spans::fill_spans`]）。
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
    use super::*;
    use lumino_editor_state::BezierAnchor;

    fn line_path(pts: &[(f32, f32)]) -> LinePath {
        pts.iter().map(|&p| BezierAnchor::new(p)).collect()
    }

    #[test]
    fn test_collect_edges_straight_uses_endpoints() {
        // 自动柄段 = 直线 → 直接用两端点，不做折线逼近
        let paths = vec![line_path(&[(0.0, 60.0), (960.0, 60.0)])];
        assert_eq!(
            collect_edges(&paths, 480.0),
            vec![((0.0, 60.0), (960.0, 60.0))]
        );
    }

    #[test]
    fn test_collect_edges_closes_nearly_touching_ends() {
        // 两条笔画接缝差 ≤ 1 格（tick ≤ snap、key ≤ 1）→ **两侧**接缝都补隐式闭合边
        let paths = vec![
            line_path(&[(0.0, 60.0), (960.0, 60.0)]),
            line_path(&[(960.0, 61.0), (0.0, 61.0)]),
        ];
        let edges = collect_edges(&paths, 480.0);
        assert_eq!(edges.len(), 4, "2 条直线 + 2 条接缝补边（左右各一）");
        for seam in [((0.0, 60.0), (0.0, 61.0)), ((960.0, 60.0), (960.0, 61.0))] {
            let closed = edges
                .iter()
                .any(|e| (e.0 == seam.0 && e.1 == seam.1) || (e.0 == seam.1 && e.1 == seam.0));
            assert!(closed, "接缝 {seam:?} 应被补上: {edges:?}");
        }
    }

    #[test]
    fn test_collect_edges_gap_beyond_tolerance_stays_open() {
        // 接缝超过容差 → 不补边（两条笔画彼此远离、各自端点也远离）
        let paths = vec![
            line_path(&[(0.0, 60.0), (960.0, 60.0)]),
            line_path(&[(2880.0, 62.0), (1920.0, 62.0)]),
        ];
        assert_eq!(collect_edges(&paths, 480.0).len(), 2, "超差接缝不封口");
    }
}
