//! 020「编辑图形」UI 测试
//!
//! 定稿交互：**必须先进入编辑图形**才出现 8+1 锚点；进入后 ShapeSelect 选框隐藏；
//! 四个角二维伸缩、四条边一维拉伸、角外 32px 旋转、中心只移动未 Bake 组合；
//! 已 Bake 不提供中心；曲线 Path 点只在曲线工具里编辑。

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{DrawnShapeSource, PendingShapeRef, ShapeNote};
use lumino_ui_core::geom::{Point, Rectangle, Size, mouse};

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
fn test_baked_shape_cannot_enter_edit_mode() {
    let mut editor = editor_with_shape();
    let notes_before: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();

    // DEC-001：Bake 后形状对象只读，已 Bake 图形不再提供 8 点手柄 / 中心点。
    assert!(!editor.enable_shape_edit(), "已 Bake 图形不得进入编辑图形");
    assert!(!editor.is_shape_edit_armed());
    let handles = editor.shape_edit_handles();
    assert!(handles.box_points.is_empty(), "已 Bake 不应画出锚点");
    assert!(handles.center.is_none());
    assert!(editor.shape_edit_box().is_none(), "已 Bake 不应有编辑框");

    let bounds = editor.editor_state.shape_select.shapes()[0]
        .source
        .bounds()
        .expect("几何");
    assert!(
        !editor.begin_shape_edit(bounds.0, bounds.3),
        "即使点在旧锚点位置也不得开始编辑"
    );
    assert!(!editor.is_shape_editing());
    let notes_after: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();
    assert_eq!(notes_after, notes_before, "拒绝编辑不得改动任何音符");
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

    // 换选到已 Bake 对象：编辑态自动失效，且不得再进入（DEC-001）
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
    assert!(
        !editor.enable_shape_edit(),
        "已 Bake 对象不得进入编辑图形（形状对象只读）"
    );
    assert!(!editor.is_shape_edit_armed());
}

#[test]
fn test_pending_shape_promotes_to_single_composed_for_edit() {
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
            filled: false,
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

    assert!(
        editor.enable_shape_edit(),
        "待确认形状应能直接进入编辑图形（内部提升为单支组合）"
    );
    assert!(editor.is_shape_edit_armed());
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert_eq!(
        editor.editor_state.pending_composed[0].kind,
        ComposedKind::Single,
        "参数化实例必须提升成可旋转的模型"
    );
    assert!(
        editor.editor_state.shape_tool.shapes.is_empty(),
        "原实例已被组合吸收"
    );

    let handles = editor.shape_edit_handles();
    assert_eq!(handles.box_points.len(), 8, "提升后仍是严格 8 个锚点");
    assert!(handles.center.is_some(), "单支组合应有中心锚点");

    // 撤销组合 → 回到参数化形状
    assert!(editor.undo_pending_compose());
    assert_eq!(editor.editor_state.shape_tool.shapes.len(), 1);
    assert!(editor.editor_state.pending_composed.is_empty());
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

/// 两个待确认单支组合：box (0..1000, 60..64) 与 (2000..3000, 60..64)
fn editor_with_two_pending_composed() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    let make = |x0: f32, x1: f32| {
        let mut model =
            lumino_core::shape::ShapeModel::new(0, 1, lumino_core::shape::ShapeKind::Custom, 1.0);
        model.geometry =
            lumino_core::shape::ShapeGeometry::from_rect(lumino_core::shape::ShapeRect {
                tick_lo: x0,
                key_lo: 60.0,
                key_hi: 64.0,
                tick_hi: x1,
            });
        model
    };
    for (x0, x1) in [(0.0, 1000.0), (2000.0, 3000.0)] {
        editor
            .editor_state
            .pending_composed
            .push(lumino_editor_state::PendingComposed::single(
                lumino_editor_state::ComposedKind::Single,
                make(x0, x1),
                Vec::new(),
                Vec::new(),
            ));
    }
    let id0 = editor.editor_state.shape_select.add_pending(
        1,
        PendingShapeRef::Composed(0),
        DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (1000.0, 64.0)],
        },
    );
    let id1 = editor.editor_state.shape_select.add_pending(
        1,
        PendingShapeRef::Composed(1),
        DrawnShapeSource::Polyline {
            points: vec![(2000.0, 60.0), (3000.0, 64.0)],
        },
    );
    editor.editor_state.shape_select.select_all_of([id0, id1]);
    editor
}

#[test]
fn test_group_edit_union_box_and_move() {
    let mut editor = editor_with_two_pending_composed();
    enter_edit(&mut editor);
    assert!(matches!(
        editor.shape_edit_armed,
        Some(ShapeEditTarget::Group)
    ));
    assert_eq!(editor.shape_group_edit.as_deref(), Some(&[0usize, 1][..]));
    let union = editor.group_union_model().expect("联合 box");
    assert_eq!(
        union.geometry.bounds().expect("bounds"),
        (0.0, 60.0, 3000.0, 64.0)
    );

    // 中心命中 = 组平移
    assert!(editor.begin_shape_edit(1500.0, 62.0), "中心应命中 Move");
    assert!(editor.update_shape_edit(1600.0, 64.0));
    assert!(editor.finish_shape_edit());
    for (index, x0) in [100.0f32, 2100.0].into_iter().enumerate() {
        let rect = editor.editor_state.pending_composed[index].results[0]
            .model
            .geometry
            .rect
            .expect("rect");
        assert!(
            (rect.tick_lo - x0).abs() < 1e-3,
            "成员 {index} 应整体 +100 tick"
        );
        assert!(
            (rect.key_lo - 62.0).abs() < 1e-3,
            "成员 {index} 应整体 +2 key"
        );
    }
    assert_eq!(editor.pending_edit_undo.len(), 1, "一次组拖动 = 一次 undo");
}

#[test]
fn test_group_resize_scales_members_about_joint_box() {
    let mut editor = editor_with_two_pending_composed();
    enter_edit(&mut editor);
    // 联合 box Se 角 = (3000, 60)，拖到 6000 ⇒ sx=2、sy=1
    assert!(editor.begin_shape_edit(3000.0, 60.0), "角点应命中");
    assert!(editor.update_shape_edit(6000.0, 60.0));
    assert!(editor.finish_shape_edit());
    let a = editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .rect
        .expect("rect");
    let b = editor.editor_state.pending_composed[1].results[0]
        .model
        .geometry
        .rect
        .expect("rect");
    assert!((a.tick_lo - 0.0).abs() < 1e-3 && (a.tick_hi - 2000.0).abs() < 1e-3);
    assert!((b.tick_lo - 4000.0).abs() < 1e-3 && (b.tick_hi - 6000.0).abs() < 1e-3);
    assert!((a.key_lo - 60.0).abs() < 1e-3 && (a.key_hi - 64.0).abs() < 1e-3);
}

#[test]
fn test_group_cancel_restores_all_members() {
    let mut editor = editor_with_two_pending_composed();
    enter_edit(&mut editor);
    let before = editor.editor_state.pending_composed.clone();
    assert!(editor.begin_shape_edit(1500.0, 62.0));
    assert!(editor.update_shape_edit(1700.0, 64.0));
    editor.cancel_shape_edit();
    assert_eq!(
        editor.editor_state.pending_composed, before,
        "取消应恢复全部成员"
    );
    assert!(editor.pending_edit_undo.is_empty(), "取消不入 undo 栈");
}

#[test]
fn test_group_edit_rejects_baked_selection() {
    let mut editor = editor_with_shape();
    let first = editor.editor_state.shape_select.selected_ids()[0];
    let second = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Polyline {
            points: vec![(2000.0, 60.0), (3000.0, 64.0)],
        },
        Vec::new(),
    );
    editor
        .editor_state
        .shape_select
        .select_all_of([first, second]);
    assert!(
        !editor.enable_shape_edit(),
        "已 Bake 多选不支持组编辑（041C 基线：仅 ✓ 前 pending）"
    );
}

#[test]
fn test_group_edit_promotes_plain_pending_shapes() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    for (x0, x1) in [(0.0, 100.0), (200.0, 300.0)] {
        editor
            .editor_state
            .shape_tool
            .shapes
            .push(lumino_editor_state::ShapeInstance {
                kind: lumino_editor_state::ShapeKind::Rectangle,
                rect: (x0, 60.0, x1, 64.0),
                shift_constrained: false,
                filled: true,
                apex_high: true,
                mask: None,
            });
    }
    editor.rebuild_pending_mirrors();
    let id0 = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("pending 0");
    let id1 = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(1))
        .expect("pending 1");
    editor.editor_state.shape_select.select_all_of([id0, id1]);

    assert!(
        editor.enable_shape_edit(),
        "多选待确认形状应先批量提升再进入组编辑"
    );
    assert!(matches!(
        editor.shape_edit_armed,
        Some(ShapeEditTarget::Group)
    ));
    assert_eq!(editor.shape_group_edit.as_deref(), Some(&[0usize, 1][..]));
    assert!(
        editor.editor_state.shape_tool.shapes.is_empty(),
        "原参数化实例应被提升移走"
    );
    assert_eq!(editor.editor_state.pending_composed.len(), 2);
    let union = editor.group_union_model().expect("联合 box");
    assert_eq!(
        union.geometry.bounds().expect("bounds"),
        (0.0, 60.0, 300.0, 64.0)
    );
}

#[test]
fn test_shift_side_handle_tilts_by_whole_keys() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let bounds = model_bounds(&editor);
    let cx = (bounds.0 + bounds.2) / 2.0;
    let before = editor.editor_state.pending_composed[0].results[0]
        .model
        .clone();

    // 不按 Shift：同一点仍是单边拉伸
    assert!(editor.begin_shape_edit(cx, bounds.1), "下边中点应命中");
    assert!(
        matches!(
            editor.shape_edit.as_ref().map(|state| state.handle),
            Some(HandleKind::Side(_))
        ),
        "不按 Shift 时下边中点应是单边拉伸"
    );
    editor.cancel_shape_edit();

    // 按住 Shift：同一点变成倾斜
    editor.set_shift_pressed(true);
    assert!(
        editor.begin_shape_edit(cx, bounds.1),
        "Shift+下边中点应命中"
    );
    let state = editor.shape_edit.as_ref().expect("应进入拖拽");
    assert!(
        matches!(state.handle, HandleKind::Skew),
        "Shift+边手柄应进入倾斜，实际 {:?}",
        state.handle
    );
    let k = state.original.k;
    assert!(editor.update_shape_edit(cx + 10.4, bounds.1));
    assert!(editor.finish_shape_edit());
    editor.set_shift_pressed(false);

    let after = editor.editor_state.pending_composed[0].results[0]
        .model
        .clone();
    let center = (bounds.1 + bounds.3) / 2.0;
    let mut shear: Option<f32> = None;
    for (old, new) in before.geometry.anchors.iter().zip(&after.geometry.anchors) {
        let dy = old.pos.1 - center;
        if dy.abs() < 1e-6 {
            continue;
        }
        let value = (new.pos.0 - old.pos.0) / dy;
        if let Some(prev) = shear {
            assert!(
                (value - prev).abs() < 1e-3,
                "同一 shear 应作用到所有行：{prev} vs {value}"
            );
        }
        shear = Some(value);
    }
    let shear = shear.expect("应有非中心行锚点参与倾斜");
    assert!(shear.abs() > 1e-6, "拖动应真的产生倾斜");
    let steps = shear / k;
    assert!(
        (steps - steps.round()).abs() < 1e-3,
        "倾斜每 key 位移应为 k={k} 的整数倍，实际 shear={shear}"
    );
}
