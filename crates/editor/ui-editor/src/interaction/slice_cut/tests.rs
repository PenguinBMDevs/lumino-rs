//! DRAW-025：切割布防与拖线交互测试

use lumino_message::Tool;

use crate::Editor;

fn editor_with_pending_rect() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.data.current_track = 1;
    editor.editor_state.tool = Tool::ShapeSelect;
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: lumino_editor_state::ShapeKind::Rectangle,
            rect: (0.0, 60.0, 100.0, 64.0),
            shift_constrained: false,
            filled: true,
            apex_high: true,
            mask: None,
        });
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(lumino_editor_state::PendingShapeRef::Shape(0))
        .expect("pending 图形应有镜像");
    editor.editor_state.shape_select.select_only(id);
    editor
}

#[test]
fn test_begin_slice_cut_requires_pending_single() {
    let mut editor = Editor::new();
    assert!(!editor.begin_slice_cut(), "空选区不应布防");
    assert!(!editor.slice_cut_active());
}

#[test]
fn test_armed_drag_slices_pending_shape() {
    let mut editor = editor_with_pending_rect();
    assert!(editor.begin_slice_cut(), "单 pending 形状应能布防");

    let start = editor.line_pos_screen_pos((50.0, 60.0));
    let end = editor.line_pos_screen_pos((50.0, 64.0));
    assert!(editor.slice_cut_pressed(start, true), "按下应开始拖线");
    assert!(editor.slice_cut_moved(end), "拖动应更新切线");
    assert!(editor.slice_cut_released(), "松手应完成切分");

    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::Slice);
    assert_eq!(composed.results.len(), 2, "应切出两片");
    assert!(
        composed.results.iter().all(|part| part.cut.is_some()),
        "两片都应带片簿记"
    );
    assert!(
        composed
            .results
            .iter()
            .all(|part| part.model.geometry.closed),
        "闭合形状切出的两片都应闭合"
    );
    assert!(!editor.slice_cut_active(), "成功后应退出布防");
}

#[test]
fn test_release_without_drag_keeps_armed() {
    let mut editor = editor_with_pending_rect();
    assert!(editor.begin_slice_cut());
    let point = editor.line_pos_screen_pos((50.0, 62.0));
    assert!(editor.slice_cut_pressed(point, true));
    // 没有移动 → 切线与零长度：切分失败，布防保留供重试
    assert!(!editor.slice_cut_released());
    assert!(editor.slice_cut_active(), "失败应保留布防");
}

#[test]
fn test_end_slice_cut_clears_arm() {
    let mut editor = editor_with_pending_rect();
    assert!(editor.begin_slice_cut());
    editor.end_slice_cut();
    assert!(!editor.slice_cut_active());
}

#[test]
fn test_closed_half_geometry_matches_its_notes() {
    // 非对称矩形 + 偏左切线：几何与音符必须配同一侧
    let mut editor = editor_with_pending_rect();
    assert!(editor.begin_slice_cut());
    let start = editor.line_pos_screen_pos((30.0, 60.0));
    let end = editor.line_pos_screen_pos((30.0, 64.0));
    assert!(editor.slice_cut_pressed(start, true));
    assert!(editor.slice_cut_moved(end));
    assert!(editor.slice_cut_released());

    let composed = &editor.editor_state.pending_composed[0];
    let (bound, other) = (
        composed.results[0]
            .model
            .geometry
            .bounds()
            .expect("有外接框"),
        composed.results[1]
            .model
            .geometry
            .bounds()
            .expect("有外接框"),
    );
    assert!(bound.2 <= 30.0 + 1e-3, "第一片几何应是小 tick 侧");
    assert!(other.0 >= 30.0 - 1e-3, "第二片几何应是大 tick 侧");
    assert!(
        composed.results[0]
            .notes
            .iter()
            .all(|note| note.tick + note.length <= 30.0 + 1e-3),
        "第一片音符也应是小 tick 侧"
    );
    assert!(
        composed.results[1]
            .notes
            .iter()
            .all(|note| note.tick >= 30.0 - 1e-3),
        "第二片音符应是大 tick 侧"
    );
}
