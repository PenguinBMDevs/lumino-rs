//! 019–022 UI 组合入口测试

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_core::shape::{BetweenRole, CopyMode, ShapeGeometry, ShapeKind, ShapeOp};
use lumino_editor_state::{DrawnShapeSource, PendingShapeRef, ShapeNote};

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

#[test]
fn test_live_then_split_restores_shapes() {
    let mut editor = seeded_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    let b = add_polyline(
        &mut editor,
        vec![(100.0, 64.0), (150.0, 64.0)],
        vec![shape_note(100.0, 64)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    assert!(editor.live_selected_shapes(), "应可折叠");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "Live 折叠不应丢文档音符"
    );
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    let live_id = editor.editor_state.shape_select.selected_ids()[0];
    assert!(
        editor
            .editor_state
            .shape_store
            .model(
                editor
                    .editor_state
                    .shape_select
                    .model_id_of(live_id)
                    .expect("有模型")
            )
            .is_some()
    );
    assert_eq!(
        editor
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .expect("选中")
            .notes
            .len(),
        2
    );

    assert!(editor.split_composed_shape(), "应可还原");
    assert_eq!(editor.editor_state.shape_select.selection_len(), 2);
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "还原 Live 不应丢文档音符"
    );
}

#[test]
fn test_join_requires_two_shapes() {
    let mut editor = empty_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(a);
    assert!(!editor.join_selected_shapes(), "单形状不能 Join");

    let b = add_polyline(
        &mut editor,
        vec![(50.0, 60.0), (100.0, 60.0)],
        vec![shape_note(100.0, 60)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    assert!(editor.join_selected_shapes());
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "Join 不应丢文档音符"
    );
    assert!(editor.split_composed_shape());
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "还原 Join 不应丢文档音符"
    );
}

#[test]
fn test_straighten_toggle_adds_and_removes_op() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (20.0, 64.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.set_straighten(true));
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(id)
        .expect("模型");
    assert!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Straighten(_)))
    );
    assert!(editor.set_straighten(false));
    assert!(
        !editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Straighten(_)))
    );
}

#[test]
fn test_symmetry_toggle_adds_and_removes_op() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.ensure_model_for_drawn(id).is_some());
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(id)
        .expect("模型");
    let mut model = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .clone();
    model.kind = ShapeKind::Curve;
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    editor.editor_state.shape_store.edit(model_id, model, 0);

    assert!(editor.toggle_symmetry(true).is_ok());
    assert!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_)))
    );
    assert!(editor.toggle_symmetry(true).is_ok());
    assert!(
        !editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_)))
    );
}

#[test]
fn test_symmetry_toggle_round_trip_keeps_geometry_and_params() {
    // DRAW-043：开 → 关 → 开 必须幂等——几何（半边锚点）任何一步都不被改写，
    // 且两次开启挂上的对称参数完全相同。
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.ensure_model_for_drawn(id).is_some());
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(id)
        .expect("模型");
    let mut seed = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .clone();
    seed.kind = ShapeKind::Curve;
    seed.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    editor.editor_state.shape_store.edit(model_id, seed, 0);

    let half = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .geometry
        .anchors
        .clone();
    let symmetry_op = |editor: &Editor| {
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .find_map(|op| match op {
                ShapeOp::Symmetry(params) => Some(*params),
                _ => None,
            })
    };

    assert!(editor.toggle_symmetry(true).is_ok());
    let first = symmetry_op(&editor).expect("第一次开启应有对称 op");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .geometry
            .anchors,
        half,
        "开启对称不得改写几何"
    );

    assert!(editor.toggle_symmetry(true).is_ok());
    assert!(symmetry_op(&editor).is_none(), "再次切换应关闭对称");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .geometry
            .anchors,
        half,
        "关闭对称后锚点必须逐点还原"
    );

    assert!(editor.toggle_symmetry(true).is_ok());
    assert_eq!(
        symmetry_op(&editor),
        Some(first),
        "重新开启应挂上与首次完全相同的对称参数"
    );
}

#[test]
fn test_slice_vertical_makes_two_shapes() {
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
    assert!(editor.slice_selected_vertical(), "应能竖切");
    assert_eq!(editor.editor_state.shape_select.selection_len(), 2);
}

#[test]
fn test_between_creates_steps() {
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
    assert!(editor.create_between_selected());
    assert_eq!(editor.editor_state.shape_select.selection_len(), 7);
}

#[test]
fn test_copy_along_adds_copies() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.copy_along_selected());
    assert_eq!(
        editor
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .expect("选中")
            .notes
            .len(),
        5,
        "原音符 + 4 副本"
    );

    // 027：fx 页必须落盘；按模型重生成（移动形状）后副本仍在
    let new_id = editor.editor_state.shape_select.selected_ids()[0];
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(new_id)
        .expect("模型");
    let mut model = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .clone();
    assert!(
        model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::CopyAlong(_))),
        "CopyAlong op 应写回模型"
    );
    for anchor in &mut model.geometry.anchors {
        anchor.pos.0 += 20.0;
    }
    assert!(editor.edit_shape(model_id, model), "按模型重估应成功");
    assert_eq!(
        editor.editor_state.shape_store.notes_of(model_id).len(),
        5,
        "移动形状重生成后副本不应消失"
    );
}

#[test]
fn test_velocity_up_writes_env_op() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.velocity_up_selected());
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(id)
        .expect("模型");
    assert!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::VelocityEnv(_)))
    );
}

#[test]
fn test_merge_two_shapes() {
    let mut editor = empty_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    let b = add_polyline(
        &mut editor,
        vec![(100.0, 60.0), (150.0, 60.0)],
        vec![shape_note(100.0, 60)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    assert!(editor.merge_selected_shapes());
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "Merge 不应丢文档音符"
    );
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
        });
}

#[test]
fn test_pending_live_composition_stays_pending_until_bake() {
    let mut editor = empty_editor();
    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    add_pending_rect(&mut editor, 0.0, 100.0, 60);
    add_pending_rect(&mut editor, 200.0, 300.0, 64);
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

    assert!(editor.live_selected_shapes(), "pending 组合应成功");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(editor.editor_state.shape_tool.shapes.is_empty());
    let pending_notes = editor.editor_state.pending_composed[0].note_count();
    assert!(pending_notes > 0);
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        0,
        "组合结果保持 pending，不写文档音符"
    );

    assert!(editor.undo(), "组合会话 undo 应回到 pending 矢量");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert_eq!(editor.editor_state.shape_tool.shapes.len(), 2);

    assert!(editor.redo(), "组合会话 redo 应重新组合");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);

    assert!(editor.confirm_shape_tool(), "✓ 才提交");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes,
        "✓ 写入的音符应与预览一致"
    );
    assert!(
        editor
            .editor_state
            .shape_select
            .pending_shapes()
            .next()
            .is_none(),
        "✓ 后不应残留 pending 镜像"
    );
    assert!(
        !editor
            .editor_state
            .shape_store
            .persisted_models()
            .is_empty(),
        "041B 只读模型应保留"
    );

    assert!(editor.undo(), "✓ 后 undo 应回到 pending 组合");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);
    assert!(editor.redo(), "✓ 后 redo 应重新烘焙");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes
    );
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

#[test]
fn test_pending_join_composition_is_pending() {
    let mut editor = empty_editor();
    setup_pending_rects(&mut editor);
    assert!(editor.join_selected_shapes());
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::Join);
    assert!(composed.note_count() > 0);
    assert!(editor.editor_state.shape_tool.shapes.is_empty());
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);
}

#[test]
fn test_pending_merge_composition_is_pending() {
    let mut editor = empty_editor();
    setup_pending_rects(&mut editor);
    assert!(editor.merge_selected_shapes());
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::Merge);
    assert!(composed.note_count() > 0);
    assert!(editor.editor_state.shape_tool.shapes.is_empty());
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);
}

#[test]
fn test_pending_slice_produces_two_parts() {
    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);

    assert!(editor.slice_selected_vertical(), "pending Slice 应成功");
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::Slice);
    assert_eq!(composed.results.len(), 2, "竖切应产生左右两片");
    let pending_notes = composed.note_count();
    assert!(pending_notes > 0);
    assert!(editor.editor_state.shape_tool.shapes.is_empty());
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);

    assert!(editor.undo(), "undo 应回到单形状");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert_eq!(editor.editor_state.shape_tool.shapes.len(), 1);
    assert!(editor.redo(), "redo 应恢复两片");

    assert!(editor.confirm_shape_tool(), "✓ 才提交两片");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes
    );
    assert_eq!(
        editor.editor_state.shape_store.persisted_models().len(),
        2,
        "两片各保留一个只读模型"
    );
}

#[test]
fn test_pending_slice_open_curve_produces_two_parts() {
    let mut editor = empty_editor();
    setup_single_pending_curve(&mut editor);

    assert!(
        editor.slice_selected_vertical(),
        "开放曲线也应能被竖切成两段"
    );
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::Slice);
    assert_eq!(composed.results.len(), 2);
    assert!(composed.note_count() > 0);
}

#[test]
fn test_pending_between_produces_first_steps_last() {
    let mut editor = empty_editor();
    setup_pending_rects(&mut editor);

    assert!(editor.create_between_selected(), "pending Between 应成功");
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::Between);
    assert_eq!(composed.results.len(), 7, "首 + 5 步 + 末");
    let roles: Vec<BetweenRole> = composed
        .results
        .iter()
        .filter_map(|part| {
            part.model.ops.iter().find_map(|op| match op {
                ShapeOp::Between(params) => Some(params.role),
                _ => None,
            })
        })
        .collect();
    assert_eq!(roles.first(), Some(&BetweenRole::First));
    assert_eq!(roles.last(), Some(&BetweenRole::Last));
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == BetweenRole::Step)
            .count(),
        5,
        "steps=5 应生成 5 个 Step"
    );
    let pending_notes = composed.note_count();
    assert!(pending_notes > 0);
    assert!(editor.editor_state.shape_tool.shapes.is_empty());
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);

    assert!(editor.undo(), "undo 应回到两个 pending 形状");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert_eq!(editor.editor_state.shape_tool.shapes.len(), 2);
    assert!(editor.redo(), "redo 应恢复补间组");

    assert!(editor.confirm_shape_tool(), "✓ 才提交补间");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes
    );
    assert_eq!(
        editor.editor_state.shape_store.persisted_models().len(),
        7,
        "首/步/末各保留一个只读模型"
    );

    assert!(editor.undo(), "✓ 后 undo 应恢复补间组");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert_eq!(editor.editor_state.pending_composed[0].results.len(), 7);
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);
    assert!(editor.redo(), "redo 应重新烘焙补间");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes
    );
}

#[test]
fn test_shape_select_shows_confirm_for_composed_results() {
    use crate::grid::shape_tool_box::shape_buttons_active;

    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);
    assert!(!shape_buttons_active(&editor), "无组合结果时不显示 ✓×");

    assert!(editor.copy_along_selected());
    assert!(
        shape_buttons_active(&editor),
        "ShapeSelect 下 composed 结果应可直接 ✓"
    );
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

#[test]
fn test_default_copy_along_uses_half_beat_step_and_four_copies() {
    let mut model = lumino_core::shape::ShapeModel::new(0, 1, ShapeKind::Polyline, 1.0);
    model.geometry = ShapeGeometry::from_points(vec![(0.0, 60.0), (40.0, 60.0)]);

    let spec = super::ops::CopyAlongSpec::half_beat(480);
    let params = Editor::default_copy_along_params(&model, spec).expect("应有路径参数");
    assert_eq!(params.mode, CopyMode::Count { n: 4 });
    assert_eq!(spec.groups, 5, "输入是总组数，不是副本数");
    assert_eq!(
        params.pts,
        vec![[1200.0, 0.0]],
        "路径长 = 半拍 × 5 段，Count 落点才等距"
    );

    let notes = [shape_note(0.0, 60)];
    let copies = lumino_editor_state::shape_ops::copy_along_copies(&notes, &params, 480.0);
    let mut ticks: Vec<f32> = copies.iter().map(|note| note.tick).collect();
    ticks.sort_by(f32::total_cmp);
    assert_eq!(
        ticks,
        vec![240.0, 480.0, 720.0, 960.0],
        "半拍步长、4 份副本（总 5 个）"
    );
}

#[test]
fn test_pending_copy_along_stays_pending_and_bakes_all_copies() {
    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);

    assert!(editor.copy_along_selected(), "pending CopyAlong 应成功");
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(
        composed.kind,
        lumino_editor_state::ComposedKind::CopyAlong,
        "027 应以 CopyAlong 组合类型留在 pending"
    );
    assert!(
        matches!(
            composed.model().expect("单支").ops.last(),
            Some(ShapeOp::CopyAlong(_))
        ),
        "CopyAlong 参数必须写回模型，后续再生成才不会丢副本"
    );
    let pending_notes = composed.note_count();
    assert!(pending_notes > 0);
    let mut ticks: Vec<f32> = composed.notes().map(|note| note.tick).collect();
    ticks.sort_by(f32::total_cmp);
    ticks.dedup();
    let step = f32::from(editor.editor_state.view.ppq) * 0.5;
    assert_eq!(
        ticks,
        vec![0.0, step, step * 2.0, step * 3.0, step * 4.0],
        "original + 4 copies at half-beat steps"
    );
    let expected_summary = format!("5 组 · 步长 {step:.0} tick");
    assert_eq!(
        editor.copy_along_summary(),
        Some(expected_summary),
        "摘要应报告总组数与 tick 步长"
    );
    assert!(
        editor.editor_state.shape_tool.shapes.is_empty(),
        "来源应从 owning tool 摘除"
    );
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        0,
        "组合结果不得提前写文档"
    );

    assert!(editor.undo(), "undo 应回到 pending 矢量");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert_eq!(editor.editor_state.shape_tool.shapes.len(), 1);
    assert!(editor.redo(), "redo 应恢复组合结果");

    assert!(editor.confirm_shape_tool(), "✓ 才提交副本");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes,
        "✓ 写入的音符数应与预览一致"
    );

    assert!(editor.undo(), "✓ 后 undo 应恢复 pending 组合");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);
    assert!(editor.redo(), "redo 应重新烘焙");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes
    );
}

#[test]
fn test_pending_copy_along_accepts_explicit_step_and_total_groups() {
    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);

    assert!(
        editor.copy_along_selected_with(120.0, 3),
        "显式步长/总组数应可用"
    );
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::CopyAlong);
    let mut ticks: Vec<f32> = composed.notes().map(|note| note.tick).collect();
    ticks.sort_by(f32::total_cmp);
    ticks.dedup();
    assert_eq!(
        ticks,
        vec![0.0, 120.0, 240.0],
        "groups=3 表示总组数 3（原图 + 2 副本），步长 120 tick"
    );
}

#[test]
fn test_pending_velocity_stays_pending_and_varies_velocity() {
    let mut editor = empty_editor();
    setup_single_pending_curve(&mut editor);

    assert!(editor.velocity_up_selected(), "pending Velocity 应成功");
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(
        composed.kind,
        lumino_editor_state::ComposedKind::Velocity,
        "028 应以 Velocity 组合类型留在 pending"
    );
    assert!(
        matches!(
            composed.model().expect("单支").ops.last(),
            Some(ShapeOp::VelocityEnv(_))
        ),
        "渐强包络必须写回模型"
    );
    let min = composed.notes().map(|note| note.velocity).min();
    let max = composed.notes().map(|note| note.velocity).max();
    assert!(
        matches!((min, max), (Some(lo), Some(hi)) if hi > lo),
        "渐强应在预览音符上产生力度差：min={min:?} max={max:?} notes={}",
        composed.note_count()
    );
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        0,
        "组合结果不得提前写文档"
    );

    let pending_notes = composed.note_count();
    assert!(editor.confirm_shape_tool());
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        pending_notes
    );
}
