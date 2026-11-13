//! 025–028：已 Bake 图形的参数操作负向测试（DEC-001）
//!
//! 已 Bake 形状只读；025 切割/026 补间/027 复制/028 力度都只对 pending 开放。

use super::*;
use lumino_editor_state::ShapeNote;

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

#[test]
fn test_slice_vertical_rejects_baked_shape() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0), (40.0, 70.0), (0.0, 70.0)],
        vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 40.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor.editor_state.shape_select.select_only(id);
    // DEC-001：Bake 即定稿，已 Bake 形状不提供切割入口
    assert!(
        !editor.slice_selected_vertical(),
        "已 Bake 形状不应再被切分"
    );
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
}

#[test]
fn test_between_rejects_baked_selection() {
    let mut editor = seeded_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    let b = add_polyline(
        &mut editor,
        vec![(100.0, 64.0), (140.0, 64.0)],
        vec![shape_note(100.0, 64)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    let before = doc_fingerprint(&editor);

    assert!(
        !editor.create_between_selected(),
        "已 Bake 图形不得创建补间"
    );
    assert_eq!(doc_fingerprint(&editor), before, "拒绝后音符必须逐条不变");
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        2,
        "拒绝后选中集不得变化"
    );
}
