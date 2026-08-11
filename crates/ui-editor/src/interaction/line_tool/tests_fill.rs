//! 颜料桶填充集成测试：填充封闭区域为实心（编辑层格点，不直接生成音符）
//!
//! 填充语义：点击封闭区域内部 → 内部格点存入 `line_tool.fill`，
//! √ 确认时与路径格点合并生成实心音符；× 清空；Ctrl+Z 撤销。

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;

/// 构造封闭矩形（两条路径围成，snap 固定 480）：
/// P1: (0,60) → (960,60) → (960,62) → (0,62)（顶 + 右 + 底）
/// P2: (0,62) → (0,60)（左侧竖线）
/// 内部格点 = 仅 (480, 61)
fn rect_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 480.0;
    editor.editor_state.line_tool.fill_enabled = true;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (0.0, 62.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
    }
    editor
}

#[test]
fn test_fill_stores_cells_not_notes() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    // 填充进入编辑层：不直接生成音符
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "填充不应直接生成音符"
    );
    assert_eq!(
        editor.editor_state.line_tool.fill,
        vec![(480.0, 61)],
        "矩形内部仅 1 格存入 fill"
    );
    assert!(editor.editor_state.line_tool.has_fill());
}

#[test]
fn test_fill_undoable() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(editor.undo(), "填充操作可撤销");
    assert!(
        !editor.editor_state.line_tool.has_fill(),
        "撤销后填充格点清空"
    );
    assert!(editor.redo(), "可重做");
    assert!(editor.editor_state.line_tool.has_fill(), "重做后填充恢复");
}

#[test]
fn test_fill_click_again_clears() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    // 再点已填充格点 → 取消全部填充
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(
        !editor.editor_state.line_tool.has_fill(),
        "再次点击已填充区域取消填充"
    );
}

#[test]
fn test_fill_on_boundary_noop() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    // 点击边界格点 (0,60) → 不填充
    editor.handle_fill_pressed(Point::new(0.0, 0.0), 0.0, 60);
    assert!(!editor.editor_state.line_tool.has_fill());
    // 点击外部 → 蔓延填充（非封闭方向），同样进入编辑层
    editor.handle_fill_pressed(Point::new(0.0, 0.0), 1440.0, 61);
    assert!(
        editor.editor_state.line_tool.has_fill(),
        "外部区域蔓延填充存入编辑层"
    );
}

#[test]
fn test_confirm_merges_path_and_fill() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    // 路径格点（矩形四边：顶 3 + 右 3 + 底 3 去重连接点 = 7，左竖线新增 1 = 8）
    // + 填充 1 格 = 9 格（无重叠）
    assert!(editor.confirm_line_tool());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 9);
    assert!(
        editor.editor_state.line_tool.paths.is_empty() && !editor.editor_state.line_tool.has_fill(),
        "确认后清空路径与填充"
    );
}

#[test]
fn test_cancel_clears_fill() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    editor.cancel_line_tool();
    assert!(!editor.editor_state.line_tool.has_fill());
    assert!(
        !editor.editor_state.line_tool.can_undo_path(),
        "取消清空历史"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
}

#[test]
fn test_fill_mode_press_does_not_create_path() {
    // fill_enabled = true 时 pressed 走填充，不创建曲线路径
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.line_tool.fill_enabled = true;
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_pressed(Point::new(120.0, 24.0), false);
    assert!(
        editor.editor_state.line_tool.paths.is_empty(),
        "填充模式点击不创建路径"
    );
}

#[test]
fn test_fill_enabled_state_lives_on_line_tool() {
    // 开关状态持久于 line_tool（Editor::set_fill_enabled 读写）
    let mut editor = Editor::new();
    assert!(!editor.fill_enabled());
    editor.set_fill_enabled(true);
    assert!(editor.fill_enabled());
    // 切到非曲线工具自动关闭
    editor.editor_state.tool = Tool::Curve;
    editor.set_tool(Tool::Pointer);
    assert!(!editor.fill_enabled(), "切走曲线工具自动关闭填充");
}

#[test]
fn test_fill_full_ui_flow_default_snap() {
    // 真实 UI 链路（默认 snap=1920）：工具栏开关 → 画布点击（handle_pressed 入口）
    // → 填充存入编辑层（不生成音符）→ √ 确认合并生成实心音符
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    // 视图参数使矩形 (0,60)-(5760,62) 位于画布可见区域：
    // x = 1920*0.25 + 键盘宽 ≈ 540、y = (127-61)*4 + 标尺 ≈ 294，均在 800x600 内
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    {
        let line = &mut editor.editor_state.line_tool;
        // 封闭矩形（1920 网格）：顶 + 右 + 底 + 左侧竖线
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (5760.0, 60.0));
        line.push_anchor(0, (5760.0, 62.0));
        line.push_anchor(0, (0.0, 62.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
    }
    seed_notes(&mut editor, 1, 0, &[]);
    // 工具栏开启颜料桶（FillToggled → set_fill_enabled）
    editor.set_fill_enabled(true);
    assert!(editor.fill_enabled());
    // 画布点击矩形内部格点 (1920,61)（handle_pressed 完整入口：snap_tick floor）
    let p = editor.line_pos_screen_pos((1920.0, 61.0));
    editor.handle_pressed(p, false);
    let mut fill = editor.editor_state.line_tool.fill.clone();
    fill.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(
        fill,
        vec![(1920.0f32, 61u16), (3840.0f32, 61u16)],
        "矩形内部 2 格 (1920,61)/(3840,61) 被填充，且不直接生成音符"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    // √ 确认：路径格点 + 填充格点合并生成实心音符
    assert!(editor.confirm_line_tool());
    assert!(editor.editor_state.data.current_track_note_count() >= 3);
}

/// 弯曲封闭图形（左侧竖线带自定义柄向上拱起 → 采样格点跳格产生边界缝隙）
fn bent_rect_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 480.0;
    editor.editor_state.line_tool.fill_enabled = true;
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (1920.0, 60.0));
        line.push_anchor(0, (1920.0, 62.0));
        line.push_anchor(0, (0.0, 62.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
    }
    // 左侧竖线弯曲拱起：采样格点 key 序列 62→67→71→73→…→60（跳格，
    // 修复前边界在 (0,61) 等位置有缝隙，泛洪双向漏穿）
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths[1][0].set_out_handle((0.0, 16.0));
        line.paths[1][1].set_in_handle((0.0, 16.0));
    }
    editor
}

#[test]
fn test_fill_bent_curve_sealed_no_leak() {
    // 弯曲封闭图形内部可填：修复前采样跳格导致泛洪漏穿到外部
    // （表现为"封闭图形填不上、背景被填"）；修复后仅填内部 3 格。
    let mut editor = bent_rect_editor();
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    let mut fill = editor.editor_state.line_tool.fill.clone();
    fill.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(
        fill,
        vec![(480.0f32, 61u16), (960.0f32, 61u16), (1440.0f32, 61u16)],
        "弯曲封闭图形内部 3 格被填充，不泄漏到外部"
    );
}

#[test]
fn test_fill_all_bent_closed_shape_sealed() {
    // 纯弯曲闭合图形（顶弧 + 底弧，无直线段）：两弧采样均跳格，
    // 缺口链直接通向外部。修复前内部点击泛洪从弧线缺口漏穿
    // （"封闭图形填不上、背景被填"）；修复后 Bresenham 补连密封。
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 1920.0;
    editor.editor_state.line_tool.fill_enabled = true;
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    {
        let line = &mut editor.editor_state.line_tool;
        // 顶弧：(0,60) → (5760,60)，柄向上拱 30 → 采样跳格
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (5760.0, 60.0));
        // 底弧：(5760,60) → (0,60)，柄向下沉 30
        line.paths.push(Vec::new());
        line.push_anchor(1, (5760.0, 60.0));
        line.push_anchor(1, (0.0, 60.0));
        // 自定义弯曲柄（push_anchor 之后设置，避免被自动重算覆盖）
        line.paths[0][0].set_out_handle((2880.0, 30.0));
        line.paths[0][1].set_in_handle((-2880.0, 30.0));
        line.paths[1][0].set_out_handle((-2880.0, -30.0));
        line.paths[1][1].set_in_handle((2880.0, -30.0));
    }
    // 点击两弧之间内部 (1920,61)
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 1920.0, 61);
    let fill = &editor.editor_state.line_tool.fill;
    assert!(!fill.is_empty(), "封闭图形内部可填充");
    assert!(
        !fill.contains(&(1920.0, 113)),
        "泛洪不得漏穿到弧线上方外部（背景不能被填）"
    );
}
