//! `drawn_shape_box` 叠加层单元测试

use super::*;

use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{DrawnShape, DrawnShapeSource, ShapeKind};

/// 非 Conductor 轨（track 1）+ 一个已登记的矩形图形（逻辑 0..4 × key 60..64）
fn editor_with_rect() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (0.0, 60.0, 4.0, 64.0),
            shift_constrained: false,
            apex_high: true,
            filled: false,
        },
        Vec::new(),
    );
    editor
}

fn polyline(points: Vec<(f32, f32)>) -> DrawnShape {
    DrawnShape {
        id: 0,
        model_id: None,
        track: 1,
        group: None,
        source: DrawnShapeSource::Polyline { points },
        notes: Vec::new(),
        hidden_by_creation: false,
        deleted: false,
        ever_deleted: false,
        moves: Vec::new(),
        pending: None,
    }
}

#[test]
fn test_outline_path_covers_shape_and_polyline_forms() {
    let editor = editor_with_rect();
    let shape = &editor.editor_state.shape_select.shapes()[0];
    assert!(
        outline_path(&editor, shape, 0.0, 0.0).is_some(),
        "形状工具图形应产生轮廓路径"
    );
    assert!(
        outline_path(&editor, &polyline(vec![(0.0, 60.0), (4.0, 64.0)]), 0.0, 0.0).is_some(),
        "折线应产生路径"
    );
    assert!(
        outline_path(&editor, &polyline(vec![(0.0, 60.0)]), 0.0, 0.0).is_some(),
        "单点笔画应退化为十字而非空路径"
    );
    assert!(
        outline_path(&editor, &polyline(Vec::new()), 0.0, 0.0).is_none(),
        "空折线不应产生路径"
    );
}

#[test]
fn test_rect_path_accepts_reversed_corners() {
    // 反向两角 → 仍能规范化成矩形路径（仅构建，不断言像素）
    let _p = rect_path(Point::new(10.0, 20.0), Point::new(2.0, 5.0));
}

#[test]
fn test_overlay_includes_selection_box_only_when_selected() {
    let mut editor = editor_with_rect();
    // 未选中且非鼠标工具 → 无内容，不产出几何
    let overlay = Overlay::collect(&editor);
    assert!(overlay.selection_box.is_none(), "未选中不应有选框");
    assert!(overlay.selected_paths.is_empty());
    assert!(overlay.is_empty(), "无内容时不应产出几何");

    // 选中后 → 有选框，叠加层非空（渲染侧不再返回 None）
    let id = editor.editor_state.shape_select.shapes()[0].id;
    editor.editor_state.shape_select.select_only(id);
    let overlay = Overlay::collect(&editor);
    let rect = overlay.selection_box.expect("选中后应有选框");
    assert!(rect.width > 0.0 && rect.height > 0.0, "选框必须有面积");
    assert_eq!(overlay.selected_paths.len(), 1, "选中高亮仍应保留");
    assert!(!overlay.is_empty(), "有选框时应产出几何");
}

#[test]
fn test_mouse_tool_shows_idle_outlines_and_marquee() {
    use lumino_message::Tool;
    let mut editor = editor_with_rect();
    // 再加一个图形，验证「常显轮廓排除已选中者」：被选中者单独走高亮
    editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (8.0, 60.0, 12.0, 64.0),
            shift_constrained: false,
            apex_high: true,
            filled: false,
        },
        Vec::new(),
    );
    editor.set_tool(Tool::ShapeSelect);
    let first = editor.editor_state.shape_select.shapes()[0].id;
    editor.editor_state.shape_select.select_only(first);
    editor
        .editor_state
        .shape_select
        .begin_marquee(0.0, 60.0, false);
    editor.editor_state.shape_select.update_marquee(20.0, 70.0);

    let overlay = Overlay::collect(&editor);
    assert_eq!(
        overlay.idle_paths.len(),
        1,
        "常显轮廓应排除已选中的那一个（它走高亮）"
    );
    assert_eq!(overlay.selected_paths.len(), 1);
    assert!(overlay.marquee_path.is_some(), "拉框中应画出拉框矩形");
    assert!(overlay.selection_box.is_some());
}

#[test]
fn test_overlay_glows_every_selected_shape_and_boxes_union() {
    use lumino_message::Tool;
    let mut editor = editor_with_rect();
    // 第二个图形（并列在右下），用于验证「多选 → 逐个高亮 + 选框取并集」
    let second = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (20.0, 40.0, 24.0, 44.0),
            shift_constrained: false,
            apex_high: true,
            filled: false,
        },
        Vec::new(),
    );
    editor.set_tool(Tool::ShapeSelect);
    let first = editor.editor_state.shape_select.shapes()[0].id;
    editor
        .editor_state
        .shape_select
        .select_all_of([first, second]);

    let overlay = Overlay::collect(&editor);
    assert_eq!(
        overlay.selected_paths.len(),
        2,
        "多选时每个选中图形都应有发光描边"
    );
    assert_eq!(overlay.idle_paths.len(), 0, "全部被选中 → 无常显轮廓");
    let rect = overlay.selection_box.expect("多选应有并集选框");
    // 并集外接框应同时包住两个图形的角点
    for c in [
        editor.line_pos_screen_pos((0.0, 60.0)),
        editor.line_pos_screen_pos((4.0, 64.0)),
        editor.line_pos_screen_pos((20.0, 40.0)),
        editor.line_pos_screen_pos((24.0, 44.0)),
    ] {
        assert!(
            rect.contains(c),
            "并集选框应包住各选中图形角点 {c:?}，实际 {rect:?}"
        );
    }
}

#[test]
fn test_edit_mode_hides_selection_box_and_shows_eight_anchors() {
    use lumino_message::Tool;
    let mut editor = editor_with_rect();
    editor.set_tool(Tool::ShapeSelect);
    let id = editor.editor_state.shape_select.shapes()[0].id;
    editor.editor_state.shape_select.select_only(id);

    let normal = Overlay::collect(&editor);
    assert!(
        normal.selection_box.is_some(),
        "普通态应有 ShapeSelect 选框"
    );
    assert!(normal.edit_box.is_none(), "普通态不应有编辑框");
    assert!(normal.edit_handles.box_points.is_empty());

    assert!(editor.enable_shape_edit(), "应能进入编辑图形");
    let overlay = Overlay::collect(&editor);
    assert!(
        overlay.selection_box.is_none(),
        "编辑态必须隐藏 ShapeSelect 选框（避免两层框重叠）"
    );
    assert!(overlay.edit_box.is_some(), "编辑态应有编辑框");
    assert_eq!(overlay.edit_handles.box_points.len(), 8, "严格 8 个锚点");
    assert!(
        overlay.edit_handles.center.is_none(),
        "已 Bake 不提供中心锚点"
    );
    assert_eq!(overlay.selected_paths.len(), 1, "选中高亮仍保留");
    assert!(!overlay.is_empty());
}

#[test]
fn test_selection_box_encloses_shape_screen_bounds() {
    let mut editor = editor_with_rect();
    let id = editor.editor_state.shape_select.shapes()[0].id;
    editor.editor_state.shape_select.select_only(id);
    let rect = editor.selection_box().expect("应有选框");
    // 图形逻辑 0..4 × 60..64 的屏幕两角都应落在选框内部（选框只增不减）
    let corners = [
        editor.line_pos_screen_pos((0.0, 60.0)),
        editor.line_pos_screen_pos((4.0, 64.0)),
    ];
    for c in corners {
        assert!(
            rect.contains(c),
            "选框应包住图形屏幕角点 {c:?}，实际 {rect:?}"
        );
    }
}
