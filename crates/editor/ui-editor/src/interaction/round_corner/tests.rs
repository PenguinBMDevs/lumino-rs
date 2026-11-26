//! DRAW-044 角度柔化模式测试

use lumino_core::shape::ShapeOp;
use lumino_editor_state::{BezierAnchor, PendingShapeRef};
use lumino_message::Tool;

use crate::Editor;

fn editor_with_pending_polyline() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.data.current_track = 1;
    editor.editor_state.tool = Tool::ShapeSelect;
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((100.0, 60.0)),
        BezierAnchor::new((100.0, 64.0)),
        BezierAnchor::new((0.0, 64.0)),
        BezierAnchor::new((0.0, 60.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线应有镜像");
    editor.editor_state.shape_select.select_only(id);
    editor
}

#[test]
fn test_begin_round_corners_requires_pending_selection() {
    let mut editor = Editor::new();
    assert!(!editor.begin_round_corners(), "空选区不应进入柔化模式");
    assert!(!editor.round_corners_active());
}

#[test]
fn test_begin_round_corners_promotes_pending_polyline() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners(), "待确认折线应能进入柔化");
    assert!(editor.round_corners_active());
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "参数化待确认形状应先提升为单支组合"
    );
    assert_eq!(
        editor.round_corner_points().len(),
        4,
        "闭合路径 5 个锚点里有 1 个与起点重合，只应给 4 个角画手柄"
    );
}

#[test]
fn test_drag_corner_sets_radius_and_undo_snapshot() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners());
    let corner = editor.line_pos_screen_pos((100.0, 60.0));
    assert!(editor.round_corner_pressed(corner), "应命中角点");
    let target = editor.line_pos_screen_pos((100.0, 66.0));
    assert!(editor.round_corner_moved(target), "拖动应更新半径");

    let model = &editor.editor_state.pending_composed[0].results[0].model;
    let params = model
        .ops
        .iter()
        .find_map(|op| match op {
            ShapeOp::Round(params) => Some(params),
            _ => None,
        })
        .expect("模型上应追加 Round 算子");
    let radius = params.radius_of(1).expect("角点 1 应有半径");
    assert!(
        (radius - 6.0).abs() < 0.5,
        "拖 6 个 key 半径应约为 6，实际 {radius}"
    );
    assert!(
        params.k > 0.0,
        "首次创建柔化应冻结 beats-per-key（否则短段圆角会被夹到看不见）"
    );

    assert!(editor.round_corner_released(), "松手应提交一次撤销");
    assert_eq!(
        editor.shape_tools.pending_edit_undo.len(),
        1,
        "一次拖拽 = 一次撤销"
    );
}

#[test]
fn test_straight_corner_is_not_offered() {
    let mut editor = Editor::new();
    editor.editor_state.data.current_track = 1;
    editor.editor_state.tool = Tool::ShapeSelect;
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((100.0, 60.0)),
        BezierAnchor::new((200.0, 60.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线应有镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.begin_round_corners());
    assert!(
        editor.round_corner_points().is_empty(),
        "共线的中间节点没有角可柔化，不应给手柄"
    );

    // 把中间节点拖出直线后，才出现可柔化角
    if let Some(part) = editor
        .editor_state
        .pending_composed
        .get_mut(0)
        .and_then(|composed| composed.results.first_mut())
    {
        part.model.geometry.anchors[1].pos.1 = 64.0;
    }
    assert_eq!(editor.round_corner_points().len(), 1);
}

#[test]
fn test_end_round_corners_clears_mode() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners());
    editor.end_round_corners();
    assert!(!editor.round_corners_active());
}

#[test]
fn test_switch_tool_exits_round_mode_and_restores_curve() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners());
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "提升为单支组合"
    );
    editor.set_tool(Tool::Curve);
    assert!(!editor.round_corners_active(), "切工具应退出柔化模式");
    assert!(
        editor.editor_state.pending_composed.is_empty(),
        "未改半径时切工具应还原提升前的曲线"
    );
    assert_eq!(
        editor.editor_state.line_tool.paths.len(),
        1,
        "曲线应回到 line_tool 容器"
    );
}

#[test]
fn test_switch_tool_keeps_changes() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners());
    let corner = editor.line_pos_screen_pos((100.0, 60.0));
    assert!(editor.round_corner_pressed(corner));
    assert!(editor.round_corner_moved(editor.line_pos_screen_pos((100.0, 70.0))));
    assert!(editor.round_corner_released());

    editor.set_tool(Tool::Curve);
    assert!(!editor.round_corners_active(), "切工具应退出柔化模式");
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "改过半径后应保留组合结果（含柔化参数）"
    );
}
#[test]
fn test_round_drag_through_entry_points() {
    let mut editor = editor_with_pending_polyline();
    editor.editor_state.canvas.size_x = 1600.0;
    editor.editor_state.canvas.size_y = 2400.0;
    assert!(editor.begin_round_corners());
    let corner = editor.line_pos_screen_pos((100.0, 60.0));
    println!("corner screen = {corner:?}");
    assert!(editor.is_inside_canvas(corner), "角点应在画布内");
    editor.handle_pressed(corner, false);
    let target = editor.line_pos_screen_pos((100.0, 70.0));
    editor.handle_moved(target);
    let model = &editor.editor_state.pending_composed[0].results[0].model;
    let params = model.ops.iter().find_map(|op| match op {
        ShapeOp::Round(params) => Some(params),
        _ => None,
    });
    println!("params after drag = {params:?}");
    assert!(params.is_some(), "拖动后应写入 Round 算子");
    editor.handle_released();
}

// ── DEBT-18：Ctrl+拖调弧形状 ──────────────────────────────────────

/// 取角点 1 的半径（模型上无 Round 或未登记时返回 None）
fn radius_of(editor: &Editor, index: usize) -> Option<f32> {
    let model = &editor.editor_state.pending_composed[0].results[0].model;
    model.ops.iter().find_map(|op| match op {
        ShapeOp::Round(params) => params.radius_of(index),
        _ => None,
    })
}

/// 取角点 1 的形状（模型上无 RoundShape 或未登记时返回 None）
fn shape_of(editor: &Editor, index: usize) -> Option<f32> {
    let model = &editor.editor_state.pending_composed[0].results[0].model;
    model.ops.iter().find_map(|op| match op {
        ShapeOp::RoundShape(params) => params.shape_of(index),
        _ => None,
    })
}

#[test]
fn test_plain_drag_does_not_add_shape_op() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners());
    let corner = editor.line_pos_screen_pos((100.0, 60.0));
    assert!(editor.round_corner_pressed(corner));
    let target = editor.line_pos_screen_pos((100.0, 66.0));
    assert!(editor.round_corner_moved(target));
    assert!(
        shape_of(&editor, 1).is_none(),
        "普通拖拽只调半径，不应追加形状算子"
    );
}

#[test]
fn test_ctrl_drag_changes_shape_without_changing_radius() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners());
    let corner = editor.line_pos_screen_pos((100.0, 60.0));
    assert!(editor.round_corner_pressed(corner));
    let target = editor.line_pos_screen_pos((100.0, 66.0));
    assert!(editor.round_corner_moved(target));
    assert!(editor.round_corner_released());
    let radius_before = radius_of(&editor, 1).expect("先拖出半径");

    editor.set_ctrl_pressed(true);
    assert!(editor.round_corner_pressed(corner));
    // 向上 2 个 key：shape = base + 2 / 半径
    let target = editor.line_pos_screen_pos((100.0, 62.0));
    assert!(editor.round_corner_moved(target));
    editor.set_ctrl_pressed(false);
    assert!(editor.round_corner_released());

    let shape = shape_of(&editor, 1).expect("Ctrl+拖应登记形状");
    assert!(shape > 0.2, "向上拖应更鼓（正形状），实际 {shape}");
    let radius_after = radius_of(&editor, 1).expect("半径仍在");
    assert!(
        (radius_after - radius_before).abs() < 1e-6,
        "Ctrl+拖不得改半径：{radius_before} → {radius_after}"
    );
    assert_eq!(
        editor.shape_tools.pending_edit_undo.len(),
        2,
        "两次拖拽 = 两次撤销"
    );
}

#[test]
fn test_ctrl_drag_on_unrounded_corner_creates_default_radius() {
    let mut editor = editor_with_pending_polyline();
    assert!(editor.begin_round_corners());
    editor.set_ctrl_pressed(true);
    let corner = editor.line_pos_screen_pos((100.0, 60.0));
    assert!(editor.round_corner_pressed(corner));
    let target = editor.line_pos_screen_pos((100.0, 61.0));
    assert!(editor.round_corner_moved(target));
    editor.set_ctrl_pressed(false);

    let radius = radius_of(&editor, 1).expect("形状拖拽应先给默认半径");
    assert!(
        (radius - 2.0).abs() < 1e-6,
        "默认半径应为 2 key，实际 {radius}"
    );
    let shape = shape_of(&editor, 1).expect("应登记形状");
    assert!(
        (shape - 0.5).abs() < 0.15,
        "向上 1 key / 半径 2 ≈ +0.5，实际 {shape}"
    );
}
