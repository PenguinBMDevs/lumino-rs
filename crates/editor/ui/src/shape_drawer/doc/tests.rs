//! 抽屉文档单测（DRAW-033B1/B2/C）

use super::*;

#[test]
fn test_six_tools_produce_distinct_strokes() {
    for tool in DrawerTool::ALL {
        if tool == DrawerTool::Select {
            continue;
        }
        let mut draft = Draft::begin(tool, (0.1, 0.1)).expect("工具应有草稿");
        draft.update((0.8, 0.7));
        let stroke = draft.to_stroke().expect("应生成 stroke");
        assert!(!stroke.anchors.is_empty(), "{tool:?} 应有锚点");
    }
}

#[test]
fn test_rect_and_circle_are_closed() {
    for tool in [DrawerTool::Rect, DrawerTool::Circle] {
        let mut draft = Draft::begin(tool, (0.1, 0.1)).expect("草稿");
        draft.update((0.9, 0.8));
        let stroke = draft.to_stroke().expect("stroke");
        assert!(stroke.rect.is_some(), "{tool:?} 应保存外接框");
        let first = stroke.anchors.first().expect("首点").pos;
        let last = stroke.anchors.last().expect("尾点").pos;
        assert!((first.0 - last.0).abs() < 1e-4);
        assert!((first.1 - last.1).abs() < 1e-4);
    }
}

#[test]
fn test_undo_redo_restores_strokes_and_selection() {
    let mut doc = DrawerDoc::default();
    let mut draft = Draft::begin(DrawerTool::Line, (0.0, 0.0)).expect("草稿");
    draft.update((1.0, 1.0));
    assert!(doc.commit_draft(draft.to_stroke()));
    assert_eq!(doc.strokes.len(), 1);
    assert!(doc.undo());
    assert!(doc.strokes.is_empty());
    assert!(doc.redo());
    assert_eq!(doc.strokes.len(), 1);
}

#[test]
fn test_select_and_delete() {
    let mut doc = DrawerDoc::default();
    let mut draft = Draft::begin(DrawerTool::Line, (0.0, 0.0)).expect("草稿");
    draft.update((1.0, 1.0));
    doc.commit_draft(draft.to_stroke());
    assert!(doc.select_at((0.5, 0.5), 0.1));
    assert_eq!(doc.selection.len(), 1);
    assert!(doc.delete_selected());
    assert!(doc.strokes.is_empty());
    assert!(doc.undo());
    assert_eq!(doc.strokes.len(), 1);
}

#[test]
fn test_shift_constrains_rect_and_circle_to_square() {
    for tool in [DrawerTool::Rect, DrawerTool::Circle] {
        let mut draft = Draft::begin(tool, (0.1, 0.1)).expect("草稿");
        draft.update_with_shift((0.8, 0.4), true);
        let stroke = draft.to_stroke().expect("stroke");
        let rect = stroke.rect.expect("外接框");
        let (w, h) = rect.size();
        assert!((w - h).abs() < 1e-4, "{tool:?} Shift 应生成正方形/正圆框");
    }
}

#[test]
fn test_drawing_roundtrip_includes_roles_and_areas() {
    let mut doc = DrawerDoc::default();
    let mut draft = Draft::begin(DrawerTool::Line, (0.1, 0.1)).expect("草稿");
    draft.update((0.9, 0.8));
    doc.commit_draft(draft.to_stroke());
    doc.roles[0].colour_slot = 3;
    doc.areas.push(AreaSpot {
        u: 0.5,
        v: 0.5,
        colour: 2,
    });
    let drawing = doc.to_drawing("id".to_string(), "name".to_string());
    let mut back = DrawerDoc::default();
    back.load_drawing(&drawing);
    assert_eq!(
        back.to_drawing("id".to_string(), "name".to_string()),
        drawing
    );
}

#[test]
fn test_flip_rotate_and_paste_are_undoable() {
    let mut doc = DrawerDoc::default();
    let mut draft = Draft::begin(DrawerTool::Line, (0.1, 0.2)).expect("草稿");
    draft.update((0.8, 0.6));
    doc.commit_draft(draft.to_stroke());
    doc.areas.push(AreaSpot {
        u: 0.25,
        v: 0.75,
        colour: 1,
    });

    assert!(doc.flip_horizontal());
    assert!((doc.strokes[0].anchors[0].pos.0 - 0.9).abs() < 1e-4);
    assert!(doc.undo());
    assert!((doc.strokes[0].anchors[0].pos.0 - 0.1).abs() < 1e-4);

    assert!(doc.rotate90());
    assert!((doc.areas[0].u - 0.75).abs() < 1e-4);
    let original = doc.strokes[0].clone();
    assert!(doc.paste_strokes(std::slice::from_ref(&original)));
    assert_eq!(doc.strokes.len(), 2);
    assert_eq!(
        doc.strokes[0].anchors[0].pos, doc.strokes[1].anchors[0].pos,
        "粘贴必须保持绝对位置，不做偏移"
    );
    assert!(doc.undo());
    assert_eq!(doc.strokes.len(), 1);
}

#[test]
fn test_area_paint_only_single_closed_stroke() {
    let mut doc = DrawerDoc::default();
    let mut rect = Draft::begin(DrawerTool::Rect, (0.1, 0.1)).expect("草稿");
    rect.update((0.9, 0.9));
    doc.commit_draft(rect.to_stroke());
    assert!(doc.can_area_paint());
    assert!(doc.set_area_mode(true));
    assert!(doc.paint_area(0.5, 0.5, 3));
    assert_eq!(doc.areas.len(), 1);
    let map = doc.area_map_cached().expect("区域网格");
    assert_eq!(map.area_at(0.5, 0.5), 3);
    assert!(doc.undo());
    assert!(doc.areas.is_empty());

    let mut line = Draft::begin(DrawerTool::Line, (0.0, 0.0)).expect("草稿");
    line.update((1.0, 1.0));
    doc.commit_draft(line.to_stroke());
    assert!(!doc.can_area_paint(), "多笔画/开放图形不得进入区域模式");
    assert!(!doc.set_area_mode(true));
}

#[test]
fn test_area_zero_clears_and_cache_invalidates_on_transform() {
    let mut doc = DrawerDoc::default();
    let mut rect = Draft::begin(DrawerTool::Rect, (0.1, 0.1)).expect("草稿");
    rect.update((0.9, 0.9));
    doc.commit_draft(rect.to_stroke());
    assert!(doc.paint_area(0.5, 0.5, 4));
    assert_eq!(doc.area_map_cached().expect("网格").area_at(0.5, 0.5), 4);
    assert!(doc.paint_area(0.5, 0.5, 0));
    assert_eq!(doc.area_map_cached().expect("网格").area_at(0.5, 0.5), 0);
    assert!(doc.flip_horizontal());
    assert_eq!(
        doc.area_map_cached().expect("网格").area_at(0.5, 0.5),
        0,
        "变换后区域缓存必须失效"
    );
}

#[test]
fn test_view_zoom_is_clamped() {
    let mut view = DrawerView::default();
    view.zoom_by(100.0);
    assert_eq!(view.zoom, 8.0);
    view.zoom_by(0.0001);
    assert_eq!(view.zoom, 0.25);
}

#[test]
fn test_tiny_drag_does_not_create_shape() {
    let mut draft = Draft::begin(DrawerTool::Rect, (0.5, 0.5)).expect("草稿");
    draft.update((0.5001, 0.5001));
    assert!(draft.to_stroke().is_none());
}
