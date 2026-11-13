//! 抽屉插入 → 待确认 / 退回抽屉（DEBT-14）

use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_core::shape::{
    DrawerInsert, ShapeDrawing, ShapeGeometry, ShapeInsertSpec, ShapeKind, ShapeModel, ShapeRect,
};

/// 带文档的编辑器（`document` 非空才可写入音符）
fn editor_with_track(track: usize) -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, track + 1, track, &[]);
    editor
}

fn straight_model(track: u16) -> ShapeModel {
    let mut model = ShapeModel::new(0, track, ShapeKind::Line, 0.25);
    model.geometry = ShapeGeometry::from_points([(960.0, 60.0), (2880.0, 72.0)]);
    model.validate().expect("测试形状应合法");
    model
}

fn drawer_origin(rect: ShapeRect) -> DrawerInsert {
    DrawerInsert::new(
        ShapeDrawing::new("case", "测试图形"),
        ShapeInsertSpec::new(4.0, 12.0, true, rect),
    )
}

#[test]
fn test_insert_pending_keeps_document_and_history_untouched() {
    let mut editor = editor_with_track(1);
    let notes_before = editor.editor_state.data.current_track_note_count();
    let history_before = editor.editor_state.data.history.undo_len();
    let rect = ShapeRect::normalized(960.0, 60.0, 2880.0, 72.0);

    assert!(editor.insert_shape_pending(straight_model(1), drawer_origin(rect)));

    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        notes_before,
        "Insert 不得写入 document（DEC-001：✓ 才是定稿）"
    );
    assert_eq!(
        editor.editor_state.data.history.undo_len(),
        history_before,
        "Insert 不得产生文档历史"
    );
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].notes().count() > 0,
        "pending 必须带预览音符（所见即所得）"
    );
    assert_eq!(
        editor.editor_state.pending_composed[0].kind,
        lumino_editor_state::ComposedKind::Insert
    );
}

#[test]
fn test_insert_pending_is_selected_for_right_click() {
    let mut editor = editor_with_track(1);
    let rect = ShapeRect::normalized(960.0, 60.0, 2880.0, 72.0);
    assert!(editor.insert_shape_pending(straight_model(1), drawer_origin(rect)));

    assert!(
        !editor.editor_state.shape_select.selected_ids().is_empty(),
        "插入后应自动选中，右键菜单才有目标"
    );
}

#[test]
fn test_return_to_drawer_removes_pending_and_returns_payload() {
    let mut editor = editor_with_track(1);
    let rect = ShapeRect::normalized(960.0, 60.0, 2880.0, 72.0);
    let notes_before = editor.editor_state.data.current_track_note_count();
    let history_before = editor.editor_state.data.history.undo_len();
    assert!(editor.insert_shape_pending(straight_model(1), drawer_origin(rect)));

    let returned = editor.return_pending_to_drawer(0).expect("应能退回抽屉");

    assert_eq!(returned.drawing.id, "case", "原图必须原样回填");
    assert!((returned.spec.beats - 4.0).abs() < f32::EPSILON);
    assert!((returned.spec.keys - 12.0).abs() < f32::EPSILON);
    assert!(returned.spec.fit_contain);
    assert!(
        editor.editor_state.pending_composed.is_empty(),
        "pending 必须消失"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        notes_before,
        "退回不得改动 document"
    );
    assert_eq!(
        editor.editor_state.data.history.undo_len(),
        history_before,
        "退回不得产生文档历史（pending 从未落盘）"
    );
}

#[test]
fn test_return_to_drawer_rejects_non_drawer_pending() {
    let mut editor = editor_with_track(1);
    // 非抽屉来源的 pending（组合入口产生）：drawer = None
    let model = straight_model(1);
    editor
        .editor_state
        .pending_composed
        .push(lumino_editor_state::PendingComposed::single(
            lumino_editor_state::ComposedKind::Single,
            model,
            Vec::new(),
            Vec::new(),
        ));

    assert!(
        editor.return_pending_to_drawer(0).is_none(),
        "非抽屉来源不得被退回（调用方因此保留 pending）"
    );
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "拒绝退回时 pending 必须保留"
    );
}

#[test]
fn test_insert_pending_rejects_conductor_track() {
    let mut editor = editor_with_track(1);
    let rect = ShapeRect::normalized(0.0, 60.0, 960.0, 72.0);
    assert!(
        !editor.insert_shape_pending(straight_model(0), drawer_origin(rect)),
        "指挥轨必须拒绝"
    );
    assert!(editor.editor_state.pending_composed.is_empty());
}

#[test]
fn test_pending_insert_is_not_in_project_snapshot() {
    let mut editor = editor_with_track(1);
    let rect = ShapeRect::normalized(960.0, 60.0, 2880.0, 72.0);
    assert!(editor.insert_shape_pending(straight_model(1), drawer_origin(rect)));

    assert!(
        editor.shapes_snapshot().is_empty(),
        "未 ✓ 的插入不得进入工程快照（保存重开后必须不存在）"
    );
}

#[test]
fn test_confirm_commits_pending_insert_and_clears_it() {
    let mut editor = editor_with_track(1);
    let rect = ShapeRect::normalized(960.0, 60.0, 2880.0, 72.0);
    let notes_before = editor.editor_state.data.current_track_note_count();
    let history_before = editor.editor_state.data.history.undo_len();

    assert!(editor.insert_shape_pending(straight_model(1), drawer_origin(rect)));
    let preview = editor.editor_state.pending_composed[0].notes().count();

    assert!(editor.confirm_shape_tool(), "✓ 应把预览音符写入文档");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        notes_before + preview,
        "✓ 后文档音符数 = 预览音符数"
    );
    assert!(
        editor.editor_state.pending_composed.is_empty(),
        "✓ 后 pending 清空"
    );
    assert_eq!(
        editor.editor_state.data.history.undo_len(),
        history_before + 1,
        "✓ 恰好产生一个撤销单元"
    );
}

#[test]
fn test_pending_insert_hit_test_accepts_note_area() {
    use lumino_core::Tool;
    use lumino_editor_state::{PendingComposed, PendingShapeRef, ShapeNote};

    let mut editor = editor_with_track(1);
    editor.set_tool(Tool::ShapeSelect);
    let rect = ShapeRect::normalized(0.0, 60.0, 960.0, 72.0);

    // 模型轮廓只在 key=60 一条线上；预览音符放在 key=66（远离轮廓）。
    // 旧实现只测轮廓 → 点在音符上打不中；新实现按音符覆盖区命中。
    let mut model = ShapeModel::new(0, 1, ShapeKind::Line, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (960.0, 60.0)]);
    let notes = vec![ShapeNote {
        track: 1,
        tick: 480.0,
        key: 66,
        length: 10.0,
        velocity: 100,
        channel: 0,
    }];
    editor
        .editor_state
        .pending_composed
        .push(PendingComposed::from_drawer(
            model,
            notes,
            drawer_origin(rect),
        ));
    editor.rebuild_pending_mirrors();

    let mirror = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Composed(0))
        .expect("待确认镜像应存在");
    assert_eq!(
        editor.hit_test_drawn_shape(480.0, 66.0),
        Some(mirror),
        "点在预览音符上（远离轮廓）也必须选中"
    );
}

#[test]
fn test_pending_right_click_works_inside_selection_box() {
    use lumino_core::Tool;
    use lumino_editor_state::PendingShapeRef;

    let mut editor = editor_with_track(1);
    // V 形：选框中心（480, 66）不在轮廓上（顶点在 key=72）
    let mut model = ShapeModel::new(0, 1, ShapeKind::Polyline, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (480.0, 72.0), (960.0, 60.0)]);
    let rect = ShapeRect::normalized(0.0, 60.0, 960.0, 72.0);
    assert!(editor.insert_shape_pending(model, drawer_origin(rect)));
    assert_eq!(
        editor.current_tool(),
        Tool::ShapeSelect,
        "插入后应处于形状选择"
    );

    let center = editor.selection_box().expect("应有选择框").center();
    assert_eq!(
        editor.pending_ref_at_screen(center),
        Some(PendingShapeRef::Composed(0)),
        "点在选框内（未命中轮廓/音符）也应把右键目标判给 pending"
    );
}
