//! 填充区域的矢量几何（渲染层）：把填充状态转换为可绘制的闭环
//!
//! 填充的视觉由**曲线几何轮廓**决定（边缘贴合实际封闭图形），与
//! snap 精度/key 间隔无关：内部填充 = 含已填格点的闭环；背景填充
//! （点击外部蔓延）= 范围矩形减去全部闭环（NonZero 填充规则下
//! 反向环构成洞）。

use super::collect_edges;
use super::loops::{assemble_loops, loop_contains_point};
use crate::Editor;

/// 填充区域的矢量几何（逻辑坐标 (tick, key)）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FillRegion {
    /// 全部闭环（点序首尾重复；背景模式作为矩形洞）
    pub all_loops: Vec<Vec<(f32, f32)>>,
    /// 含 ≥1 个已填格点的闭环（需要显示的内部填充）
    pub filled_loops: Vec<Vec<(f32, f32)>>,
    /// 是否存在背景填充（蔓延到范围边界）
    pub has_background: bool,
    /// 已填格点外接范围（半格扩边，背景矩形边界）
    pub bounds: (f32, f32, f32, f32),
}

/// 计算填充区域的矢量几何（渲染层用；点击判定/音符生成仍走 `fill_cells`）
pub(crate) fn fill_region(editor: &Editor) -> Option<FillRegion> {
    let line = &editor.editor_state.line_tool;
    if !line.has_fill() {
        return None;
    }
    let snap = editor.editor_state.view.snap_precision.max(1.0);
    let edges = collect_edges(&line.paths, snap);
    let all_loops = assemble_loops(&edges);
    // 背景判定：存在不被任何闭环包含的已填格点（无闭环时全部是背景）
    let has_background = all_loops.is_empty()
        || line.fill.iter().any(|&(t, k)| {
            all_loops
                .iter()
                .all(|lp| !loop_contains_point(lp, t, k as f32))
        });
    // 需要显示的内部闭环：包含 ≥1 个已填格点
    let filled_loops = all_loops
        .iter()
        .filter(|lp| {
            line.fill
                .iter()
                .any(|&(t, k)| loop_contains_point(lp, t, k as f32))
        })
        .cloned()
        .collect();
    // 已填格点外接范围（半格扩边）
    let mut min_t = f32::MAX;
    let mut max_t = f32::MIN;
    let mut min_k = f32::MAX;
    let mut max_k = f32::MIN;
    for &(t, k) in &line.fill {
        min_t = min_t.min(t);
        max_t = max_t.max(t);
        min_k = min_k.min(k as f32);
        max_k = max_k.max(k as f32);
    }
    if min_t == f32::MAX {
        return None;
    }
    Some(FillRegion {
        all_loops,
        filled_loops,
        has_background,
        bounds: (
            min_t - snap * 0.5,
            min_k - 0.5,
            max_t + snap * 0.5,
            max_k + 0.5,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::fill_region;
    use crate::Editor;
    use lumino_core::Tool;

    /// 构造两条路径围合成的"近闭合矩形"（接缝差 1 key，由端点容差补边闭合）：
    /// 路径 1：key 60 顶边 (0,60)→(4,60)；路径 2：key 61 底边 (4,61)→(0,61)。
    /// 端点 (4,60)-(4,61) 差 1 key、(0,60)-(0,61) 差 1 key → 补边成环。
    fn rect_editor(fill: &[(f32, u16)]) -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        editor.editor_state.view.snap_precision = 1.0;
        {
            let line = &mut editor.editor_state.line_tool;
            line.paths.push(Vec::new());
            line.push_anchor(0, (0.0, 60.0));
            line.push_anchor(0, (4.0, 60.0));
            line.paths.push(Vec::new());
            line.push_anchor(1, (4.0, 61.0));
            line.push_anchor(1, (0.0, 61.0));
            line.add_fill_cells(fill);
        }
        editor
    }

    #[test]
    fn test_fill_region_interior_loop() {
        // 填充格点在环内 → 内部模式：filled 环 = 该闭环，无背景
        let editor = rect_editor(&[(2.0, 60)]);
        let region = fill_region(&editor).expect("有填充");
        assert_eq!(region.all_loops.len(), 1, "接缝补边闭合为一个环");
        assert_eq!(region.filled_loops.len(), 1, "环包含已填格点 → 显示");
        assert!(!region.has_background, "内部填充无背景");
        // 起点取决于组装顺序，断言顶点集合
        let mut verts = region.all_loops[0].clone();
        verts.pop();
        verts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            verts,
            vec![(0.0, 60.0), (0.0, 61.0), (4.0, 60.0), (4.0, 61.0)],
            "环 = 两条曲线 + 端点容差补边围成的矩形"
        );
        let lp = &region.all_loops[0];
        assert_eq!(lp[0], lp[lp.len() - 1], "首尾闭合");
    }

    #[test]
    fn test_fill_region_background_spread() {
        // 填充格点在环外 → 背景模式：filled 环为空，has_background
        let editor = rect_editor(&[(100.0, 100)]);
        let region = fill_region(&editor).expect("有填充");
        assert!(region.has_background, "环外格点 = 背景蔓延");
        assert!(region.filled_loops.is_empty(), "无内部环需要显示");
        assert_eq!(region.bounds, (99.5, 99.5, 100.5, 100.5), "外接范围扩半格");
    }

    #[test]
    fn test_fill_region_no_fill_none() {
        let editor = rect_editor(&[]);
        assert!(fill_region(&editor).is_none(), "无填充 → 无区域");
    }

    #[test]
    fn test_fill_region_mixed_interior_and_background() {
        // 混合：环内 + 环外格点 → 背景 + 内部环同时显示
        let editor = rect_editor(&[(2.0, 60), (100.0, 100)]);
        let region = fill_region(&editor).expect("有填充");
        assert!(region.has_background, "存在环外格点 → 有背景");
        assert_eq!(region.filled_loops.len(), 1, "环内格点 → 内部环显示");
    }
}
