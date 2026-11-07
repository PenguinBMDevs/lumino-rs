//! 019–028 UI 组合入口测试（按职责拆分）

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{DrawnShapeSource, PendingShapeRef, ShapeNote};

mod drawn;
mod params;
mod pending;
mod pending_ops;

fn shape_note(tick: f32, key: u16) -> ShapeNote {
    ShapeNote {
        track: 1,
        tick,
        key,
        length: 10.0,
        velocity: 100,
        channel: 0,
    }
}

fn add_polyline(editor: &mut Editor, points: Vec<(f32, f32)>, notes: Vec<ShapeNote>) -> u64 {
    editor
        .editor_state
        .shape_select
        .add(1, None, DrawnShapeSource::Polyline { points }, notes)
}

fn seeded_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(
        &mut editor,
        2,
        1,
        &[Note::new(0.0, 60, 10.0), Note::new(100.0, 64, 10.0)],
    );
    editor
}

fn empty_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor
}

fn add_pending_rect(editor: &mut Editor, tick0: f32, tick1: f32, key: u16) {
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
            mask: None,
        });
}

fn setup_pending_rects(editor: &mut Editor) {
    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    add_pending_rect(editor, 0.0, 100.0, 60);
    add_pending_rect(editor, 200.0, 300.0, 64);
    editor.rebuild_pending_mirrors();
    let ids: Vec<u64> = (0..2)
        .filter_map(|index| {
            editor
                .editor_state
                .shape_select
                .pending_mirror(PendingShapeRef::Shape(index))
        })
        .collect();
    editor.editor_state.shape_select.select_all_of(ids);
}

fn setup_single_pending_rect(editor: &mut Editor) {
    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    add_pending_rect(editor, 0.0, 100.0, 60);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("单个 pending 图形应有镜像");
    editor.editor_state.shape_select.select_only(id);
}

/// 单条 pending 曲线：音符沿 tick 分布，028 的力度包络才有可观察的渐强。
fn setup_single_pending_curve(editor: &mut Editor) {
    use lumino_editor_state::BezierAnchor;

    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((200.0, 72.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("单条 pending 曲线应有镜像");
    editor.editor_state.shape_select.select_only(id);
}
