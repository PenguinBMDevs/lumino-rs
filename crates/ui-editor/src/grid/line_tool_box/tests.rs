//! 曲线工具盒（锚点/曲线/控制柄/矢量填充/悬浮按钮）渲染层测试

use super::*;
use lumino_core::Tool;

/// 构造曲线工具 + 两条完整路径的编辑器（默认视图 128 键 × 20px、画布 800x600）
///
/// 路径 1：key 105..110（y 364..464）与 tick 5000..5300（x 620..650）；
/// 路径 2：key 90..95（y 764..864——超出画布但几何计算不受影响）。
fn multi_curve_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (5000.0, 105.0));
        line.push_anchor(0, (5300.0, 110.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (5400.0, 90.0));
        line.push_anchor(1, (5700.0, 95.0));
    }
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor
}

/// 构造两个"近闭合矩形"图形（A：tick 0..4 key 60..61；B：tick 10..14 key 60..61），
/// 接缝差 1 key → 端点容差补边成环
fn two_rects_editor(fill: &[(f32, u16)]) -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 1.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    {
        let line = &mut editor.editor_state.line_tool;
        // A
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (4.0, 60.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (4.0, 61.0));
        line.push_anchor(1, (0.0, 61.0));
        // B
        line.paths.push(Vec::new());
        line.push_anchor(2, (10.0, 60.0));
        line.push_anchor(2, (14.0, 60.0));
        line.paths.push(Vec::new());
        line.push_anchor(3, (14.0, 61.0));
        line.push_anchor(3, (10.0, 61.0));
        line.add_fill_marks(fill);
    }
    editor
}

/// 带符号环绕数（半开区间，与填充判定一致）
fn loop_wn(pts: &[(f32, f32)], px: f32, py: f32) -> i32 {
    let mut wn = 0;
    for w in pts.windows(2) {
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
    wn
}

/// 模拟 `build_fill_path` 的 NonZero 绕数合计（矩形 +1、反向环 -1、已填环 +1）
fn render_wn(region: &FillRegion, pt: (f32, f32)) -> i32 {
    let mut w = 0;
    if region.has_background {
        let (min_t, min_k, max_t, max_k) = region.bounds;
        let rect = [
            (min_t, min_k),
            (max_t, min_k),
            (max_t, max_k),
            (min_t, max_k),
            (min_t, min_k),
        ];
        w += loop_wn(&rect, pt.0, pt.1);
        for lp in &region.all_loops {
            w -= loop_wn(lp, pt.0, pt.1);
        }
    }
    for lp in &region.filled_loops {
        w += loop_wn(lp, pt.0, pt.1);
    }
    w
}

#[test]
fn test_button_rects_centered_on_all_paths() {
    // 两条路径：按钮垂直中心 = 两条路径包围盒中心
    let editor = multi_curve_editor();
    let btns = line_button_rects(&editor).expect("按钮应存在");
    let (min_x, max_x, min_y, max_y) = paths_bounds(&editor).expect("包围盒应存在");

    let mid_y = (min_y + max_y) * 0.5;
    let btn_center_y = btns.confirm.y + BUTTON_SIZE * 0.5;
    assert!(
        (btn_center_y - mid_y).abs() < 1.0,
        "按钮应垂直居中于全部路径包围盒（mid_y {mid_y} vs center {btn_center_y}）"
    );
    let mid_x = (min_x + max_x) * 0.5;
    assert!(btns.confirm.x >= mid_x, "按钮应在包围盒右侧");
    // 按钮完整位于内容区内
    let content = content_bounds(&editor);
    for rect in [btns.confirm, btns.cancel] {
        assert!(rect.x >= content.x);
        assert!(rect.y >= content.y);
        assert!(rect.x + rect.width <= content.x + content.width);
        assert!(rect.y + rect.height <= content.y + content.height);
    }
}

#[test]
fn test_button_rects_none_for_other_tool() {
    let mut editor = multi_curve_editor();
    editor.editor_state.tool = Tool::Pencil;
    assert!(line_button_rects(&editor).is_none());
}

#[test]
fn test_button_rects_none_without_complete_path() {
    // 只有一条未完整路径 → 不显示按钮
    let mut editor = multi_curve_editor();
    editor.editor_state.line_tool.paths[1].clear();
    editor.editor_state.line_tool.paths[0].truncate(1);
    assert!(line_button_rects(&editor).is_none());
}

/// 蓝军验证：矢量填充的 NonZero 绕数合计 = 填充显示区域
#[test]
fn test_fill_render_winding_interior_only() {
    // 只填 A 内部 → 无背景：A 内绕数 1（显示），B 内 0（不显示）
    let editor = two_rects_editor(&[(2.0, 60)]);
    let region = fill_region(&editor).expect("有填充");
    assert!(!region.has_background);
    assert_eq!(render_wn(&region, (2.0, 60.0)), 1, "已填图形内部显示");
    assert_eq!(render_wn(&region, (12.0, 60.0)), 0, "未填图形内部不显示");
}

#[test]
fn test_fill_render_winding_background_holes() {
    // 填 A 内部 + 背景一点 → 背景模式：
    // 背景区域 = 1（显示）；B 内部 = 0（洞，未点过）；A 内部 = 1（已填）
    let editor = two_rects_editor(&[(2.0, 60), (100.0, 100)]);
    let region = fill_region(&editor).expect("有填充");
    assert!(region.has_background);
    assert_eq!(render_wn(&region, (50.0, 80.0)), 1, "背景区域显示");
    assert_eq!(render_wn(&region, (2.0, 60.0)), 1, "已填图形内部显示");
    assert_eq!(
        render_wn(&region, (12.0, 60.0)),
        0,
        "未填图形内部 = 洞（不显示）"
    );
}

/// 冒烟：`build_fill_path` 在两种模式（内部/背景+洞）下均可构建（不 panic）
#[test]
fn test_build_fill_path_smoke() {
    let editor = two_rects_editor(&[(2.0, 60)]);
    let region = fill_region(&editor).expect("有填充");
    let _ = build_fill_path(&editor, &region);

    let editor = two_rects_editor(&[(2.0, 60), (100.0, 100)]);
    let region = fill_region(&editor).expect("有填充");
    assert!(region.has_background);
    let _ = build_fill_path(&editor, &region);
}
