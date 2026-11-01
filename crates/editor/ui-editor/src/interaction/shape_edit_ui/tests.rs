//! 020「编辑图形」UI 测试
//!
//! 定稿交互：**必须先进入编辑图形**才出现 8+1 锚点；进入后 ShapeSelect 选框隐藏；
//! 四个角二维伸缩、四条边一维拉伸、角外 32px 旋转、中心只移动未 Bake 组合；
//! 已 Bake 不提供中心；曲线 Path 点只在曲线工具里编辑。

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use iced_core::{Point, Rectangle, Size, mouse};
use lumino_editor_state::{DrawnShapeSource, PendingShapeRef, ShapeNote};

/// 已 Bake 的折线图形（屏幕 100px 宽 × 80px 高，锚点不会互相重叠）
pub(super) fn editor_with_shape() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[Note::new(0.0, 60, 10.0)]);
    let id = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (1000.0, 64.0)],
        },
        vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 10.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.ensure_model_for_drawn(id).is_some());
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor
}

pub(super) fn editor_with_pending_composed() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: lumino_editor_state::ShapeKind::Rectangle,
            rect: (0.0, 60.0, 1000.0, 64.0),
            shift_constrained: false,
            filled: true,
            apex_high: true,
            mask: None,
        });
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("pending 镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.copy_along_selected(), "先组合成单支 pending");
    editor
}

pub(super) fn enter_edit(editor: &mut Editor) {
    assert!(editor.enable_shape_edit(), "应能进入编辑图形");
    assert!(editor.is_shape_edit_armed(), "进入后应处于编辑态");
}

pub(super) fn model_bounds(editor: &Editor) -> (f32, f32, f32, f32) {
    editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .bounds()
        .expect("几何")
}

pub(super) fn screen_box(editor: &Editor, bounds: (f32, f32, f32, f32)) -> Rectangle {
    let a = editor.line_pos_screen_pos((bounds.0, bounds.1));
    let b = editor.line_pos_screen_pos((bounds.2, bounds.3));
    Rectangle::new(
        Point::new(a.x.min(b.x), a.y.min(b.y)),
        Size::new((b.x - a.x).abs(), (b.y - a.y).abs()),
    )
}

#[test]
fn test_edit_mode_is_required() {
    let mut editor = editor_with_pending_composed();
    let bounds = model_bounds(&editor);
    assert!(
        !editor.begin_shape_edit(bounds.0, bounds.3),
        "未进入编辑图形时，锚点不得抢占整组拖动"
    );
    assert!(!editor.is_shape_editing());

    enter_edit(&mut editor);
    assert!(
        editor.begin_shape_edit(bounds.0, bounds.3),
        "进入编辑图形后点角锚点应开始伸缩"
    );
    assert!(editor.is_shape_editing());
}

#[test]
fn test_strict_eight_plus_center_for_pending() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let handles = editor.shape_edit_handles();
    assert_eq!(handles.box_points.len(), 8, "严格 8 个箱体锚点");
    assert!(handles.center.is_some(), "未 Bake 组合应有中心锚点");
}

#[test]
fn test_baked_has_eight_handles_without_center() {
    let mut editor = editor_with_shape();
    enter_edit(&mut editor);
    let handles = editor.shape_edit_handles();
    assert_eq!(handles.box_points.len(), 8, "已 Bake 仍是 8 个箱体锚点");
    assert!(
        handles.center.is_none(),
        "已 Bake 的中心移动必须禁用（Bake 后音符权威）"
    );

    let rect = editor.shape_edit_box().expect("编辑框");
    let center = editor.pos_to_tick(Point::new(
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
    ));
    let center_key = editor.pos_to_raw_key(Point::new(
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
    ));
    assert!(
        !editor.begin_shape_edit(center, center_key),
        "编辑框中心不应命中任何锚点"
    );
}

#[test]
fn test_path_anchors_are_not_edit_handles() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor.editor_state.line_tool.paths.push(vec![
        lumino_editor_state::BezierAnchor::new((0.0, 60.0)),
        lumino_editor_state::BezierAnchor::new((500.0, 63.0)),
        lumino_editor_state::BezierAnchor::new((1000.0, 60.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.copy_along_selected(), "曲线应能组合成单支");

    enter_edit(&mut editor);
    let handles = editor.shape_edit_handles();
    assert_eq!(handles.box_points.len(), 8, "曲线也只显示 8 个箱体锚点");
    assert!(handles.center.is_some());
    assert_eq!(
        handles.box_points.len() + usize::from(handles.center.is_some()),
        9,
        "Path 点不得混入编辑态手柄"
    );
}

#[test]
fn test_pending_center_moves_whole_group() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let before = model_bounds(&editor);
    let notes_before = editor.editor_state.pending_composed[0].note_count();
    assert!(notes_before > 0);

    let cx = (before.0 + before.2) / 2.0;
    let cy = (before.1 + before.3) / 2.0;
    assert!(editor.begin_shape_edit(cx, cy), "中心锚点应命中 Move");
    assert!(editor.update_shape_edit(cx + 4.0, cy + 1.0));
    assert!(editor.finish_shape_edit());

    let after = model_bounds(&editor);
    assert!((after.0 - (before.0 + 4.0)).abs() < 1e-3, "整体右移");
    assert!((after.1 - (before.1 + 1.0)).abs() < 1e-3, "整体上移");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        0,
        "中心平移不得写文档"
    );
    assert_eq!(
        editor.editor_state.pending_composed[0].note_count(),
        notes_before,
        "预览音符应整体跟随"
    );
}

#[test]
fn test_shape_handle_cursor_feedback() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let (x0, y0, x1, y1) = model_bounds(&editor);
    let nw = editor.line_pos_screen_pos((x0, y1));
    assert_eq!(
        editor.shape_handle_interaction(nw),
        Some(mouse::Interaction::ResizingDiagonallyDown),
        "左上角应为对角缩放光标"
    );
    let left = editor.line_pos_screen_pos((x0, (y0 + y1) / 2.0));
    assert_eq!(
        editor.shape_handle_interaction(left),
        Some(mouse::Interaction::ResizingHorizontally),
        "左边中点应为水平拉伸光标"
    );
    let center = editor.line_pos_screen_pos(((x0 + x1) / 2.0, (y0 + y1) / 2.0));
    assert_eq!(
        editor.shape_handle_interaction(center),
        Some(mouse::Interaction::Move),
        "中心锚点应为移动光标"
    );
    let rect = screen_box(&editor, (x0, y0, x1, y1));
    let rotate = Point::new(rect.x + rect.width + 12.0, rect.y - 12.0);
    assert_eq!(
        editor.shape_handle_interaction(rotate),
        Some(mouse::Interaction::Grab),
        "角外应为旋转光标"
    );
}

#[test]
fn test_cursor_is_ignored_outside_edit_mode() {
    let editor = editor_with_pending_composed();
    let (x0, _, _, y1) = model_bounds(&editor);
    assert_eq!(
        editor.shape_handle_interaction(editor.line_pos_screen_pos((x0, y1))),
        None,
        "未进入编辑图形时不接管光标"
    );
}

#[test]
fn test_pending_handle_points_round_trip_to_hit_test() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let handles = editor.shape_edit_handles();
    let mut points = handles.box_points;
    points.extend(handles.center);
    assert_eq!(points.len(), 9, "8+1");
    for point in points {
        let tick = editor.pos_to_tick(point);
        let key = editor.pos_to_raw_key(point);
        assert!(
            editor.begin_shape_edit(tick, key),
            "屏幕锚点 {point:?} 应能命中"
        );
    }
}

#[test]
fn test_enable_shape_edit_toggles_and_recovers_from_stale_target() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    assert!(editor.enable_shape_edit(), "再次点击应退出编辑图形");
    assert!(!editor.is_shape_edit_armed());
    assert!(editor.enable_shape_edit(), "退出后可再次进入");
    assert!(editor.is_shape_edit_armed());

    // 换选到另一个对象：编辑态自动失效，选框回来
    let second = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (1000.0, 64.0)],
        },
        Vec::new(),
    );
    editor.editor_state.shape_select.select_only(second);
    assert!(!editor.is_shape_edit_armed(), "换选后不应残留旧编辑目标");
    assert!(editor.enable_shape_edit(), "对新对象仍能进入编辑图形");
    assert!(editor.is_shape_edit_armed());
}

#[test]
fn test_switching_tool_exits_edit_mode() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    editor.set_tool(lumino_message::Tool::Curve);
    assert!(!editor.is_shape_edit_armed(), "离开鼠标工具应退出编辑图形");
    assert!(editor.shape_edit_handles().box_points.is_empty());
    assert!(editor.shape_edit_box().is_none());
}
