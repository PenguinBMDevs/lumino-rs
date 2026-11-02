//! Mask 编辑器接线测试

use lumino_core::shape::{MaskParams, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp};
use lumino_editor_state::ShapeNote;

use super::*;
use crate::tests::test_helpers::seed_notes;

fn test_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 3, 1, &[]);
    editor
}

fn mask_model() -> ShapeModel {
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    model.geometry = geometry;
    model.push_op(ShapeOp::Mask(MaskParams {
        size_keys: 0.0,
        own: false,
        ..MaskParams::default()
    }));
    model
}

fn subject_model() -> ShapeModel {
    ShapeModel::new(0, 1, ShapeKind::Line, 0.25)
}

fn note(track: usize) -> ShapeNote {
    ShapeNote {
        track,
        tick: 0.0,
        key: 61,
        length: 480.0,
        velocity: 100,
        channel: 0,
    }
}

fn pending_rect(editor: &mut Editor, tick0: f32, tick1: f32, key: u16, mask: Option<MaskParams>) {
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: lumino_editor_state::ShapeKind::Rectangle,
            rect: (tick0, key as f32, tick1, key as f32 + 4.0),
            shift_constrained: false,
            filled: true,
            apex_high: true,
            mask,
        });
}

#[test]
fn test_pending_mask_preview_ghosts_and_visible_without_doc_notes() {
    let mut editor = test_editor();
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    pending_rect(&mut editor, 0.0, 480.0, 60, None);
    pending_rect(
        &mut editor,
        200.0,
        300.0,
        60,
        Some(MaskParams {
            size_keys: 0.0,
            own: false,
            ..MaskParams::default()
        }),
    );

    let previews = editor.mask_previews();
    assert!(!previews.is_empty(), "pending 遮罩应产出预览");
    let ghosts: usize = previews.iter().map(|preview| preview.ghosts.len()).sum();
    let visible: usize = previews.iter().map(|preview| preview.visible.len()).sum();
    assert!(ghosts > 0, "ghost 保留原音符");
    assert!(visible > 0, "遮罩内应有可见片段");
    assert!(visible <= ghosts);
    assert_eq!(
        editor.editor_state.data.track_notes(1).len(),
        0,
        "pending 遮罩只预览，不写文档"
    );
    assert!(
        !editor.mask_outline_notes().is_empty(),
        "own=false 的 pending mask 应提供虚线轮廓"
    );
}

#[test]
fn test_mask_edit_target_and_pending_params_round_trip() {
    let mut editor = test_editor();
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    // 被遮对象 + 遮罩对象：成对时才会出现 ghost/visible 预览
    pending_rect(&mut editor, 0.0, 480.0, 60, None);
    pending_rect(&mut editor, 0.0, 200.0, 60, None);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(1))
        .expect("pending 镜像");
    editor.editor_state.shape_select.select_only(id);

    assert_eq!(
        editor.mask_edit_target(),
        Some(MaskEditTarget::Pending { index: 1 }),
        "待确认形状应走 Pending 目标"
    );
    assert_eq!(
        editor.mask_params_of(MaskEditTarget::Pending { index: 1 }),
        None
    );
    assert!(editor.mask_previews().is_empty(), "未启用时无预览");

    let params = MaskParams {
        size_keys: 4.0,
        invert: true,
        own: false,
        ..MaskParams::default()
    };
    assert!(editor.set_pending_mask(1, Some(params.clone())));
    assert_eq!(
        editor.mask_params_of(MaskEditTarget::Pending { index: 1 }),
        Some(params.clone()),
        "面板读回同一份参数"
    );
    assert!(!editor.mask_previews().is_empty(), "参数生效后应有预览");

    assert!(editor.set_pending_mask(1, None));
    assert_eq!(
        editor.mask_params_of(MaskEditTarget::Pending { index: 1 }),
        None
    );
    assert!(editor.mask_previews().is_empty(), "关掉后预览清空");
    assert_eq!(
        editor.editor_state.data.track_notes(1).len(),
        0,
        "面板改参数不得写文档"
    );
}

#[test]
fn test_mask_edit_target_reads_baked_params() {
    let mut editor = test_editor();
    let model_id = editor.editor_state.shape_store.insert(mask_model(), 0);
    let shape_id = editor.editor_state.shape_select.add(
        1,
        None,
        lumino_editor_state::DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)],
        },
        Vec::new(),
    );
    editor
        .editor_state
        .shape_select
        .set_model_id(shape_id, Some(model_id));
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor.editor_state.shape_select.select_only(shape_id);

    assert_eq!(
        editor.mask_edit_target(),
        Some(MaskEditTarget::Baked { model_id }),
        "已 Bake 形状应走 Baked 目标"
    );
    let params = editor
        .mask_params_of(MaskEditTarget::Baked { model_id })
        .expect("应读到 Mask op");
    assert!(!params.own, "读回的是模型里存的那份参数");
}

#[test]
fn test_toggle_pending_mask_marks_and_unmarks_selected_shape() {
    let mut editor = test_editor();
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    pending_rect(&mut editor, 0.0, 250.0, 60, None);
    pending_rect(&mut editor, 0.0, 480.0, 60, None);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("pending 镜像");
    editor.editor_state.shape_select.select_only(id);

    assert!(editor.toggle_mask_selected());
    assert!(editor.editor_state.shape_tool.shapes[0].mask.is_some());
    assert!(!editor.mask_previews().is_empty(), "标记后应有预览");

    assert!(editor.toggle_mask_selected());
    assert!(editor.editor_state.shape_tool.shapes[0].mask.is_none());
    assert!(editor.mask_previews().is_empty(), "取消后预览清空");
}

#[test]
fn test_confirm_records_pending_mask_op_in_readonly_model() {
    let mut editor = test_editor();
    pending_rect(&mut editor, 0.0, 480.0, 60, Some(MaskParams::default()));
    assert!(editor.confirm_shape_tool(), "✓ 应写入音符");
    assert!(
        editor
            .editor_state
            .shape_store
            .persisted_models()
            .iter()
            .any(|model| model.ops.iter().any(|op| matches!(op, ShapeOp::Mask(_)))),
        "只读模型应保留 Mask op"
    );
}

#[test]
fn test_preview_returns_ghost_and_visible_without_touching_notes() {
    let mut editor = test_editor();
    let mask_id = editor.editor_state.shape_store.insert(mask_model(), 0);
    let subject_id = editor.editor_state.shape_store.insert(subject_model(), 0);
    let subject_notes = vec![note(1)];
    editor
        .editor_state
        .shape_store
        .assign_notes(subject_id, subject_notes.clone());
    editor.write_shape_notes(0, &subject_notes);

    let previews = editor.mask_previews();
    assert_eq!(previews.len(), 1, "只应为被遮形状产出一份预览");
    assert_eq!(previews[0].ghosts.len(), 1, "ghost 保留原音符");
    assert_eq!(previews[0].visible.len(), 1, "inside 区域透出原音符");
    assert_eq!(
        editor.editor_state.shape_store.notes_of(subject_id),
        subject_notes.as_slice(),
        "预览不改 ShapeStore 音符"
    );
    assert_eq!(
        editor.editor_state.data.track_notes(1).len(),
        1,
        "底层音符总数不变"
    );
    assert!(editor.editor_state.shape_store.model(mask_id).is_some());
}

#[test]
fn test_apply_mask_creates_new_shape_and_keeps_originals() {
    let mut editor = test_editor();
    let mask_id = editor.editor_state.shape_store.insert(mask_model(), 0);
    let subject_id = editor.editor_state.shape_store.insert(subject_model(), 0);
    let subject_notes = vec![note(1)];
    editor
        .editor_state
        .shape_store
        .assign_notes(subject_id, subject_notes.clone());
    editor.write_shape_notes(0, &subject_notes);

    let new_id = editor.apply_mask(mask_id).expect("应产出新形状");
    assert_ne!(new_id, mask_id);
    let store = &editor.editor_state.shape_store;
    assert_eq!(store.notes_of(new_id).len(), 1);
    assert_eq!(store.notes_of(new_id)[0].key, 61);
    assert_eq!(
        store.notes_of(subject_id),
        subject_notes.as_slice(),
        "被遮形状不变"
    );
    assert!(
        store
            .model(mask_id)
            .expect("遮罩仍在")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Mask(_))),
        "遮罩不变"
    );
    assert_eq!(
        editor.editor_state.data.track_notes(1).len(),
        2,
        "原音符 + Apply 新音符"
    );
}

#[test]
fn test_silent_mask_exposes_outline_notes_only() {
    let mut editor = test_editor();
    assert!(editor.create_shape(mask_model()));
    let id = *editor
        .editor_state
        .shape_store
        .ids()
        .last()
        .expect("应有遮罩 id");
    assert!(
        !editor.mask_outline_notes().is_empty(),
        "own=false 应有灰色边框示意"
    );
    assert!(editor.set_mask(
        id,
        Some(MaskParams {
            own: true,
            ..MaskParams::default()
        })
    ));
    assert!(
        editor.mask_outline_notes().is_empty(),
        "own=true 的边框走正常音符，不重复叠加"
    );
    assert!(
        editor
            .editor_state
            .shape_store
            .entry(id)
            .expect("形状应存在")
            .source_notes
            .is_empty(),
        "切回发声后应清掉旧 source，区域改读 notes"
    );
}

#[test]
fn test_set_mask_toggles_marker() {
    let mut editor = test_editor();
    let mut model = mask_model();
    model.ops.clear();
    assert!(editor.create_shape(model), "基础形状应登记");
    let id = *editor
        .editor_state
        .shape_store
        .ids()
        .last()
        .expect("应有形状 id");

    assert!(editor.set_mask(
        id,
        Some(MaskParams {
            own: false,
            ..MaskParams::default()
        })
    ));
    assert!(
        lumino_editor_state::shape_ops::masks::params_of(
            editor
                .editor_state
                .shape_store
                .model(id)
                .expect("形状应存在")
        )
        .is_some()
    );

    assert!(editor.set_mask(id, None));
    assert!(
        lumino_editor_state::shape_ops::masks::params_of(
            editor
                .editor_state
                .shape_store
                .model(id)
                .expect("形状应存在")
        )
        .is_none()
    );
}

#[test]
fn test_own_false_keeps_source_notes_but_writes_nothing() {
    let mut editor = test_editor();
    assert!(editor.create_shape(mask_model()), "遮罩形状应登记");
    let mask_id = *editor
        .editor_state
        .shape_store
        .ids()
        .last()
        .expect("应有遮罩 id");
    assert!(
        editor.editor_state.data.track_notes(1).is_empty(),
        "own=false 的遮罩不应写入发声音符"
    );
    assert!(
        !editor
            .editor_state
            .shape_store
            .source_notes_of(mask_id)
            .is_empty(),
        "区域计算所需的边框音符仍应保留"
    );
}

#[test]
fn test_apply_mask_keeps_source_track() {
    let mut editor = test_editor();
    let mask_id = editor.editor_state.shape_store.insert(mask_model(), 0);
    let subject_id = editor.editor_state.shape_store.insert(subject_model(), 0);
    let subject_notes = vec![note(2)];
    editor
        .editor_state
        .shape_store
        .assign_notes(subject_id, subject_notes.clone());
    editor.write_shape_notes(0, &subject_notes);

    let new_id = editor.apply_mask(mask_id).expect("应产出新形状");
    assert_eq!(editor.editor_state.shape_store.notes_of(new_id)[0].track, 2);
    assert_eq!(editor.editor_state.data.track_notes(2).len(), 2);

    assert!(editor.delete_shape(new_id), "Apply 形状应可删除");
    assert_eq!(
        editor.editor_state.data.track_notes(2).len(),
        1,
        "删除跨轨 Apply 形状应清掉它写入的音符"
    );
}

#[test]
fn test_mask_is_never_masked() {
    let mut editor = test_editor();
    let mask_id = editor.editor_state.shape_store.insert(mask_model(), 0);
    editor.editor_state.shape_store.assign_notes(
        mask_id,
        vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 480.0,
            velocity: 100,
            channel: 0,
        }],
    );
    assert!(
        editor.mask_previews().is_empty(),
        "遮罩自身不应产出 ghost/visible 预览"
    );
}

#[test]
fn test_no_mask_returns_no_preview_or_apply() {
    let mut editor = test_editor();
    let subject_id = editor.editor_state.shape_store.insert(subject_model(), 0);
    editor
        .editor_state
        .shape_store
        .assign_notes(subject_id, vec![note(1)]);
    assert!(editor.mask_previews().is_empty());
    assert!(editor.apply_mask(subject_id).is_none(), "非遮罩不能 Apply");
}
