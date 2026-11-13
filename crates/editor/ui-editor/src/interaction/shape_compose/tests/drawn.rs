//! 019–024：已 Bake 图形只读（DEC-001）负向测试
//!
//! Bake 后音符是唯一权威，所有组合/参数入口都必须拒绝已 Bake 选区，
//! 且不得改动文档音符或 ShapeStore 模型。

use super::*;
use lumino_editor_state::shape_ops::SymReject;

/// 文档音符指纹：拒绝写入后必须逐条不变
fn doc_fingerprint(editor: &Editor) -> Vec<(u32, u8, u32, u8, u8)> {
    editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|note| {
            (
                note.start_tick,
                note.key,
                note.end_tick,
                note.velocity,
                note.channel,
            )
        })
        .collect()
}

fn two_baked_shapes(editor: &mut Editor) {
    let a = add_polyline(
        editor,
        vec![(0.0, 60.0), (50.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    let b = add_polyline(
        editor,
        vec![(100.0, 64.0), (150.0, 64.0)],
        vec![shape_note(100.0, 64)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
}

#[test]
fn test_compose_entries_reject_baked_selection() {
    let mut editor = seeded_editor();
    two_baked_shapes(&mut editor);
    let before = doc_fingerprint(&editor);
    let selection_before = editor.editor_state.shape_select.selection_len();

    assert!(!editor.live_selected_shapes(), "已 Bake 不得再折叠为 Live");
    assert!(!editor.join_selected_shapes(), "已 Bake 不得再连接");
    assert!(!editor.merge_selected_shapes(), "已 Bake 不得再合并");
    assert!(!editor.create_between_selected(), "已 Bake 不得创建补间");
    assert!(!editor.split_composed_shape(), "已 Bake 不提供拆分");

    assert_eq!(doc_fingerprint(&editor), before, "拒绝后音符必须逐条不变");
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        selection_before,
        "拒绝后选中集不得变化"
    );
}

#[test]
fn test_copy_along_rejects_baked_selection() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    let before = doc_fingerprint(&editor);

    assert!(!editor.copy_along_selected(), "已 Bake 不得沿路径复制");
    assert!(
        !editor.copy_along_selected_with(120.0, 3),
        "显式参数入口也必须拒绝"
    );
    assert_eq!(doc_fingerprint(&editor), before, "拒绝后音符必须逐条不变");
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "选中集不得变化"
    );
}

#[test]
fn test_velocity_up_rejects_baked_selection() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    let before = doc_fingerprint(&editor);

    assert!(!editor.velocity_up_selected(), "已 Bake 不得应用力度包络");
    assert_eq!(doc_fingerprint(&editor), before, "拒绝后音符必须逐条不变");
}

#[test]
fn test_straighten_and_symmetry_reject_baked_selection() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (20.0, 64.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    let model_id = editor.ensure_model_for_drawn(id).expect("只读镜像模型");
    let ops_before = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .ops
        .clone();
    let notes_before = doc_fingerprint(&editor);

    assert!(!editor.set_straighten(true), "已 Bake 不得扶正");
    assert!(!editor.set_straighten(false), "关闭扶正也不得回写");
    assert_eq!(
        editor.toggle_symmetry(true),
        Err(SymReject::NoSelection),
        "已 Bake 不得对称"
    );
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops,
        ops_before,
        "拒绝后 ShapeStore 算子链不得变化"
    );
    assert_eq!(doc_fingerprint(&editor), notes_before, "拒绝后音符不得变化");
}
