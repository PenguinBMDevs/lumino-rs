//! 027/028：Bake 前组合（沿路径复制 / 力度渐强）测试

use super::*;
use lumino_core::shape::{CopyMode, ShapeGeometry, ShapeKind, ShapeOp};

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
fn test_pending_symmetry_stays_pending_and_undoes() {
    use lumino_editor_state::BezierAnchor;

    let mut editor = empty_editor();
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((100.0, 70.0)),
        BezierAnchor::new((200.0, 60.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线镜像");
    editor.editor_state.shape_select.select_only(id);

    // DEC-001：对称只对 pending 开放；普通 pending 曲线先提升为单支组合。
    assert!(
        editor.toggle_symmetry(true).is_ok(),
        "pending 曲线应支持半边对称"
    );
    let composed = &editor.editor_state.pending_composed[0];
    assert!(
        composed
            .model()
            .expect("单支")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_))),
        "对称参数应写回 pending 模型"
    );
    assert!(composed.note_count() > 0, "预览音符应重算");
    assert!(
        editor.editor_state.data.current_track_notes().is_empty(),
        "pending 对称不得写文档"
    );

    assert!(editor.undo(), "撤销组合应回到 pending 曲线");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert_eq!(editor.editor_state.line_tool.paths.len(), 1);
    assert!(editor.editor_state.data.current_track_notes().is_empty());
}

#[test]
fn test_pending_symmetry_rejects_non_curve() {
    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);
    assert_eq!(
        editor.toggle_symmetry(true),
        Err(lumino_editor_state::shape_ops::SymReject::NotCurve),
        "参数化矩形不是曲线，必须拒绝"
    );
    assert!(
        editor.editor_state.pending_composed.is_empty()
            || editor.editor_state.pending_composed[0]
                .model()
                .is_none_or(|model| {
                    !model
                        .ops
                        .iter()
                        .any(|op| matches!(op, ShapeOp::Symmetry(_)))
                }),
        "拒绝后不得写入对称参数"
    );
}

#[test]
fn test_pending_symmetry_changes_preview_notes_and_undoes_once() {
    use lumino_editor_state::BezierAnchor;

    let mut editor = empty_editor();
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    // 非对称曲线：右半段明显不是左半的镜像
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((100.0, 70.0)),
        BezierAnchor::new((200.0, 64.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.promote_pending_for_edit(), "先提升为单支组合");

    let before_notes = editor.editor_state.pending_composed[0].results[0]
        .notes
        .clone();
    assert!(editor.toggle_symmetry(true).is_ok(), "对称应成功");

    let after = &editor.editor_state.pending_composed[0].results[0];
    assert!(
        after
            .model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_))),
        "对称参数应挂到 pending 模型"
    );
    assert_ne!(after.notes, before_notes, "对称必须改变预览音符");
    assert!(
        editor.editor_state.data.current_track_notes().is_empty(),
        "pending 对称不得写文档"
    );

    assert!(editor.undo(), "一次 undo 只回退这次对称");
    assert_eq!(
        editor.editor_state.pending_composed[0].results[0].notes, before_notes,
        "undo 后预览音符应还原"
    );
}

#[test]
fn test_pending_horizontal_symmetry_changes_preview() {
    use lumino_editor_state::BezierAnchor;

    let mut editor = empty_editor();
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((100.0, 68.0)),
        BezierAnchor::new((200.0, 65.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.promote_pending_for_edit(), "先提升为单支组合");
    let before = editor.editor_state.pending_composed[0].results[0]
        .notes
        .clone();

    assert!(editor.toggle_symmetry(false).is_ok(), "横轴对称应成功");
    let after = &editor.editor_state.pending_composed[0].results[0];
    assert!(
        after
            .model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_))),
        "横轴对称参数应挂到 pending 模型"
    );
    assert_ne!(after.notes, before, "横轴对称必须改变预览音符");
    assert!(editor.editor_state.data.current_track_notes().is_empty());
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
