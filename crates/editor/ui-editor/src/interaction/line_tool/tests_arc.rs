//! 三点圆弧模式（DRAW-014）：落点、预览与确认

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;
use lumino_core::shape::ShapeKind;

/// 曲线工具 + 固定 ppq/zoom 的编辑器（圆弧模式开启）
fn arc_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.ppq = 480;
    editor.editor_state.view.zoom_x = 0.1;
    editor.editor_state.view.zoom_y = 10.0;
    editor.set_line_arc_mode(true);
    editor
}

/// 在逻辑坐标 (tick, key) 处按一下（屏幕坐标由同一映射反推）
fn press(editor: &mut Editor, tick: f32, key: f32) {
    let pos = editor.line_pos_screen_pos((tick, key));
    editor.handle_line_tool_pressed(pos, tick, key);
}

#[test]
fn test_three_presses_build_one_arc_and_fourth_starts_new() {
    let mut editor = arc_editor();
    press(&mut editor, 0.0, 60.0);
    press(&mut editor, 480.0, 66.0);
    press(&mut editor, 960.0, 60.0);

    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths.len(), 1, "三点只应产生一条路径");
    assert_eq!(line.paths[0].len(), 3);
    assert_eq!(line.paths[0][0].pos, (0.0, 60.0));
    assert!(
        (line.paths[0][1].pos.0 - 480.0).abs() < 1.0
            && (line.paths[0][1].pos.1 - 66.0).abs() < 1e-3,
        "经过点应保持自由位置，实际 {:?}",
        line.paths[0][1].pos
    );
    assert_eq!(line.paths[0][2].pos, (960.0, 60.0));
    assert!(line.is_complete());
    assert_eq!(line.arc_draft, None, "满 3 点后收尾");

    press(&mut editor, 1440.0, 60.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths.len(), 2, "第四次点击开始新弧");
    assert_eq!(line.paths[1].len(), 1);
}

#[test]
fn test_arc_preview_uses_sampled_points_not_straight_anchors() {
    let mut editor = arc_editor();
    let path = vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((960.0, 70.0)),
        BezierAnchor::new((3840.0, 60.0)),
    ];
    editor.editor_state.line_tool.paths = vec![path.clone()];

    let arc_blocks = editor.path_outline_blocks(&path);
    editor.set_line_arc_mode(false);
    let straight_blocks = editor.path_outline_blocks(&path);

    assert!(!arc_blocks.is_empty(), "弧预览应产出音符块");
    assert_ne!(
        arc_blocks, straight_blocks,
        "弧预览必须走 arc_points_scaled，而不是三点直连"
    );
}

#[test]
fn test_confirm_arc_writes_notes_and_records_arc_model() {
    let mut editor = arc_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    press(&mut editor, 0.0, 60.0);
    press(&mut editor, 480.0, 66.0);
    press(&mut editor, 960.0, 60.0);

    assert!(editor.confirm_line_tool(), "完整弧应能确认生成音符");
    assert!(
        editor.editor_state.data.current_track_note_count() > 0,
        "应写入音符"
    );
    assert!(editor.editor_state.line_tool.paths.is_empty(), "确认后清空");
    assert!(
        editor.editor_state.line_tool.arc_mode,
        "圆弧模式是模式设置，确认后保留"
    );

    let entry = editor
        .editor_state
        .shape_store
        .iter()
        .find(|entry| entry.model.kind == ShapeKind::Arc)
        .expect("应登记 Arc 只读模型");
    assert_eq!(entry.model.geometry.anchors.len(), 3, "模型保留原始三点");
    assert!(entry.model.k > 0.0, "应落盘绘制时的 k");
    assert!(!entry.notes.is_empty(), "模型应绑定生成的音符");
}

#[test]
fn test_confirm_collinear_arc_falls_back_without_panic() {
    let mut editor = arc_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    press(&mut editor, 0.0, 60.0);
    press(&mut editor, 480.0, 60.0);
    press(&mut editor, 960.0, 60.0);

    assert!(editor.confirm_line_tool(), "共线三点应回退为折线");
    assert!(
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .all(|n| n.key == 60),
        "共线三点应保持直线"
    );
}

#[test]
fn test_incomplete_arc_is_not_confirmable() {
    let mut editor = arc_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    press(&mut editor, 0.0, 60.0);
    press(&mut editor, 480.0, 66.0);
    assert!(
        !editor.editor_state.line_tool.is_complete(),
        "两点不是完整圆弧，√ 不应出现"
    );
}

#[test]
fn test_arc_k_frozen_when_view_zooms() {
    let mut editor = arc_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    press(&mut editor, 0.0, 60.0);
    press(&mut editor, 480.0, 66.0);
    press(&mut editor, 960.0, 60.0);

    let frozen = editor
        .editor_state
        .line_tool
        .arc_k
        .expect("首个弧点应冻结 k");
    let path = editor.editor_state.line_tool.paths[0].clone();
    let before = editor.line_arc_points(&path).expect("三点弧应可采样");

    // 缩放视图：数据形状必须保持不变（形状优先，k 不重算）
    editor.editor_state.view.zoom_y *= 4.0;
    let after = editor.line_arc_points(&path).expect("缩放后仍应可采样");
    assert_eq!(before, after, "缩放视图不应改变弧的采样点");
    assert!(
        (editor.line_arc_k() - f64::from(frozen)).abs() < 1e-6,
        "line_arc_k 必须继续使用冻结值"
    );

    // √ 落盘的 k 也必须是绘制时的值，而不是缩放后的视图比例
    assert!(editor.confirm_line_tool());
    let entry = editor
        .editor_state
        .shape_store
        .iter()
        .find(|entry| entry.model.kind == ShapeKind::Arc)
        .expect("应登记 Arc 模型");
    assert!(
        (entry.model.k - frozen).abs() < 1e-3,
        "模型 k 应为绘制时冻结值 {frozen}，实际 {}",
        entry.model.k
    );
}

#[test]
fn test_arc_anchor_can_be_dragged_after_completion() {
    let mut editor = arc_editor();
    press(&mut editor, 0.0, 60.0);
    press(&mut editor, 480.0, 66.0);
    press(&mut editor, 960.0, 60.0);

    let start = editor.editor_state.line_tool.paths[0][1].pos;
    let screen = editor.line_pos_screen_pos(start);
    editor.handle_line_tool_pressed(screen, start.0, start.1);
    assert!(
        matches!(
            editor.editor_state.line_tool.interaction,
            LineToolInteraction::DraggingAnchor { path: 0, idx: 1 }
        ),
        "点击已完成弧的锚点应进入拖动，而不是开始新弧"
    );

    editor.handle_line_tool_moved(0.0, 0.0, start.0 + 120.0, start.1 + 4.0);
    let moved = editor.editor_state.line_tool.paths[0][1].pos;
    assert!(
        (moved.0 - (start.0 + 120.0)).abs() < 1.0 && (moved.1 - (start.1 + 4.0)).abs() < 1e-3,
        "经过点应自由跟随拖动，实际 {moved:?}"
    );

    editor.handle_line_tool_released();
    assert_eq!(
        editor.editor_state.line_tool.interaction,
        LineToolInteraction::SelectedAnchor { path: 0, idx: 1 }
    );
    assert!(editor.editor_state.line_tool.undo_path());
    assert!(
        (editor.editor_state.line_tool.paths[0][1].pos.0 - start.0).abs() < 1e-3,
        "一次 Ctrl+Z 应还原拖动前的锚点"
    );
}
