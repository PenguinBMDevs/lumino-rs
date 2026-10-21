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
    }
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
