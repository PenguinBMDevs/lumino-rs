//! 曲线工具盒（锚点/曲线/控制柄/矢量填充/悬浮按钮）渲染层测试

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;
use lumino_editor_state::{BezierAnchor, LineToolInteraction};

/// 构造曲线工具 + 两条完整路径的编辑器（默认视图 128 键 × 20px、画布 800x600）
///
/// 路径 1：key 105..110（y 364..464）与 tick 5000..5300（x 620..650）；
/// 路径 2：key 90..95（y 764..864——超出画布但几何计算不受影响）。
fn multi_curve_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 2, 1, &[]);
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

/// 纵向卷帘回归（BUG：曲线工具纵向无法使用）：完整路径下按钮必须存在，
/// 且完整落在**纵向**内容区（全宽 × 画布高−键盘高）内。
/// 旧 `content_bounds` 横向语义会把按钮钳到不存在的标尺/键盘列上。
#[test]
fn test_button_rects_vertical_roll_inside_vertical_content() {
    let mut editor = multi_curve_editor();
    editor.editor_state.is_vertical_roll = true;
    // 路径锚点 (5000,105)/(5300,110) 纵向屏幕 Y 为负（tick 超出视口），
    // 但按钮钳制逻辑必须仍产出内容区内位置（与横向越界行为一致）
    let btns = line_button_rects(&editor).expect("纵向模式按钮应存在");
    let content = content_bounds(&editor);
    assert_eq!(
        (content.x, content.y),
        (0.0, 0.0),
        "纵向内容区应从画布原点开始"
    );
    for rect in [btns.confirm, btns.cancel] {
        assert!(rect.x >= content.x, "按钮不得超出内容区左缘");
        assert!(rect.y >= content.y, "按钮不得超出内容区顶缘");
        assert!(
            rect.x + rect.width <= content.x + content.width,
            "按钮不得超出内容区右缘"
        );
        assert!(
            rect.y + rect.height <= content.y + content.height,
            "按钮不得压到底部键盘区"
        );
    }
}

#[test]
fn test_bend_buttons_require_selected_middle_anchor() {
    let mut editor = multi_curve_editor();
    assert!(
        line_bend_button_rects(&editor).is_none(),
        "未选中锚点时不显示弯折按钮"
    );
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths[0].insert(1, BezierAnchor::new((5150.0, 107.0)));
        line.interaction = LineToolInteraction::SelectedAnchor { path: 0, idx: 1 };
    }
    let btns = line_bend_button_rects(&editor).expect("中间锚点应显示弯折按钮");
    assert_eq!(btns.current, lumino_editor_state::AnchorBend::Smooth);
    assert!(!btns.smooth.contains(Point::new(0.0, 0.0)));

    editor.editor_state.line_tool.interaction =
        LineToolInteraction::SelectedAnchor { path: 0, idx: 0 };
    assert!(
        line_bend_button_rects(&editor).is_none(),
        "端点不可切换弯折"
    );
}
