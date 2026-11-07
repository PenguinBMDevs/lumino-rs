//! Mask Apply 实体化测试（从 masks/tests.rs 拆出）

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
