//! 填充区域的矢量几何（渲染层）：把填充状态转换为可绘制的闭环
//!
//! 填充的视觉由**曲线几何轮廓**决定（边缘贴合实际封闭图形），与
//! snap 精度/key 间隔无关：内部填充 = 含标记格点的闭环；背景填充
//! （标记在环外）= 可见范围矩形减去全部闭环（NonZero 填充规则下
//! 反向环构成洞）。
//!
//! 环覆盖判定 = 标记/格点**中心** vs 闭环绕数（中心在环内 ∨ 距边
//! < 半格），与 √ 确认时的音符计算（`confirm_fill_cells`）同规则 →
//! 填充显示与音符覆盖永远一致。

use super::collect_edges;
use super::loops::{assemble_loops, loop_covers_cell};
use crate::Editor;

/// 填充区域的矢量几何（逻辑坐标 (tick, key)）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FillRegion {
    /// 全部闭环（点序首尾重复；背景模式作为矩形洞）
    pub all_loops: Vec<Vec<(f32, f32)>>,
    /// 含 ≥1 个标记格点的闭环（需要显示的内部填充）
    pub filled_loops: Vec<Vec<(f32, f32)>>,
    /// 是否存在背景填充（蔓延到可见范围边界）
    pub has_background: bool,
    /// 背景矩形范围（tick/key 逻辑坐标；= 画布可见范围，与 √ 计算一致）
    pub bounds: (f32, f32, f32, f32),
}

/// 计算填充区域的矢量几何（渲染层用；标记来自 `line_tool.fill`）
pub(crate) fn fill_region(editor: &Editor) -> Option<FillRegion> {
    let line = &editor.editor_state.line_tool;
    if !line.has_fill() {
        return None;
    }
    let snap = editor.editor_state.view.snap_precision.max(1.0);
    let edges = collect_edges(&line.paths, snap);
    let all_loops = assemble_loops(&edges);
    // 格点中心判定（与 √ 计算同规则 → 边框一致）
    let covered = |lp: &Vec<(f32, f32)>, m: (f32, u16)| -> bool {
        loop_covers_cell(lp, m.0 + snap * 0.5, m.1 as f32 + 0.5, snap)
    };
    // 背景判定：存在不被任何闭环覆盖的标记（无闭环时全部是背景）
    let has_background = all_loops.is_empty()
        || line
            .fill
            .iter()
            .any(|&m| all_loops.iter().all(|lp| !covered(lp, m)));
    // 需要显示的内部闭环：覆盖 ≥1 个标记
    let filled_loops = all_loops
        .iter()
        .filter(|lp| line.fill.iter().any(|&m| covered(lp, m)))
        .cloned()
        .collect();
    // 背景矩形 = 画布可见 tick 区间 × 全键盘 key（与 confirm_fill_cells 范围一致，纵向转置）
    let (tick_lo, tick_hi) = if editor.editor_state.is_vertical_roll {
        let view = &editor.editor_state.view;
        let canvas_h = editor.editor_state.canvas.size_y;
        let kb_h = view.keyboard_width;
        let grid_h = (canvas_h - kb_h).max(0.0);
        let lo = (view.scroll_x / view.zoom_x).max(0.0);
        let hi = ((view.scroll_x + grid_h) / view.zoom_x).max(lo + snap);
        (lo, hi)
    } else {
        let lo = editor.x_to_tick(0.0).max(0.0);
        let hi = editor
            .x_to_tick(editor.editor_state.canvas.size_x)
            .max(lo + snap);
        (lo, hi)
    };
    let key_count = editor.editor_state.view.key_count;
    let bounds = (tick_lo, 0.0, tick_hi, key_count.saturating_sub(1) as f32);
    Some(FillRegion {
        all_loops,
        filled_loops,
        has_background,
        bounds,
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
    /// 视图固定：snap=1、zoom_x=0.25、画布 800x600、128 键
    /// → 可见 tick 范围 [0, (800-120)/0.25 = 2720]。
    fn rect_editor(fill: &[(f32, u16)]) -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        editor.editor_state.view.snap_precision = 1.0;
        editor.editor_state.view.zoom_x = 0.25;
        editor.editor_state.view.zoom_y = 4.0;
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        {
            let line = &mut editor.editor_state.line_tool;
            line.paths.push(Vec::new());
            line.push_anchor(0, (0.0, 60.0));
            line.push_anchor(0, (4.0, 60.0));
            line.paths.push(Vec::new());
            line.push_anchor(1, (4.0, 61.0));
            line.push_anchor(1, (0.0, 61.0));
            line.add_fill_marks(fill);
        }
        editor
    }

    #[test]
    fn test_fill_region_interior_loop() {
        // 标记在环内 → 内部模式：filled 环 = 该闭环，无背景
        let editor = rect_editor(&[(2.0, 60)]);
        let region = fill_region(&editor).expect("有填充");
        assert_eq!(region.all_loops.len(), 1, "接缝补边闭合为一个环");
        assert_eq!(region.filled_loops.len(), 1, "环含标记 → 显示");
        assert!(!region.has_background, "内部填充无背景");
        // 起点取决于组装顺序，断言顶点集合
        let mut verts = region.all_loops[0].clone();
        verts.pop();
        verts.sort_by(|a, b| a.partial_cmp(b).expect("顶点坐标比较不应为 NaN"));
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
        // 标记在环外 → 背景模式：filled 环为空，has_background
        // 背景矩形 = 可见范围（[0,0]-[3200,127]），与 √ 计算一致
        let editor = rect_editor(&[(100.0, 100)]);
        let region = fill_region(&editor).expect("有填充");
        assert!(region.has_background, "环外标记 = 背景蔓延");
        assert!(region.filled_loops.is_empty(), "无内部环需要显示");
        assert_eq!(
            region.bounds,
            (0.0, 0.0, 2720.0, 127.0),
            "背景矩形 = 可见范围"
        );
    }

    #[test]
    fn test_fill_region_no_fill_none() {
        let editor = rect_editor(&[]);
        assert!(fill_region(&editor).is_none(), "无填充 → 无区域");
    }

    #[test]
    fn test_fill_region_mixed_interior_and_background() {
        // 混合：环内 + 环外标记 → 背景 + 内部环同时显示
        let editor = rect_editor(&[(2.0, 60), (100.0, 100)]);
        let region = fill_region(&editor).expect("有填充");
        assert!(region.has_background, "存在环外标记 → 有背景");
        assert_eq!(region.filled_loops.len(), 1, "环内标记 → 内部环显示");
    }
}
