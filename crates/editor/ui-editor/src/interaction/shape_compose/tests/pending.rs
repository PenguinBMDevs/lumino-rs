//! 041C：Bake 前组合（019–026）测试

use super::*;
use lumino_core::shape::{BetweenRole, ShapeOp};

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
fn test_slice_then_bake_undo_restores_sliced_state_not_path_history() {
    use lumino_editor_state::BezierAnchor;

    let mut editor = empty_editor();
    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((200.0, 72.0)),
        BezierAnchor::new((400.0, 64.0)),
    ]);
    // 模拟真实绘制：锚点追加会写入路径历史；切割消费曲线后该历史必须重建。
    editor.editor_state.line_tool.push_path_history();
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线镜像");
    editor.editor_state.shape_select.select_only(id);
    let anchors_before = editor.editor_state.line_tool.paths[0].len();
    assert_eq!(anchors_before, 3, "测试曲线应含中间锚点");

    assert!(editor.slice_selected_vertical(), "先切割");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(editor.confirm_shape_tool(), "再 Bake");

    assert!(editor.undo(), "Bake undo 应回到切割后的组合");
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "应回到两片组合，而不是回放曲线路径历史"
    );
    assert!(
        editor.editor_state.line_tool.paths.is_empty(),
        "曲线已被组合消费，不应被路径历史复活"
    );
    assert!(
        !editor.editor_state.line_tool.can_undo_path(),
        "组合后路径历史应已重建，不能抢在文档 undo 之前"
    );
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);

    assert!(editor.undo(), "再 undo 才回到切割前的曲线");
    assert_eq!(
        editor.editor_state.line_tool.paths.len(),
        1,
        "应恢复原始曲线"
    );
    assert_eq!(
        editor.editor_state.line_tool.paths[0].len(),
        anchors_before,
        "应恢复全部锚点，不能丢中间节点"
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

#[test]
fn test_pending_mould_requires_exactly_two_shapes() {
    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);
    // 没有刀（切到鼠标工具前没选音符）→ 报 NoWalls，而不是笼统的选区错误
    assert_eq!(
        editor.mould_selected(),
        Err(crate::MouldReject::NoWalls),
        "单形状但没有刀时应提示先选音符"
    );
    assert!(
        editor.editor_state.pending_composed.is_empty(),
        "失败不应产生组合结果"
    );
}

#[test]
fn test_pending_mould_uses_selected_notes_as_knife() {
    let mut editor = Editor::new();
    // 文档里两条同 key、中间留空的音符 = 刀
    seed_notes(
        &mut editor,
        2,
        1,
        &[Note::new(0.0, 60, 100.0), Note::new(200.0, 60, 100.0)],
    );
    editor.editor_state.interaction.selected_notes = [0_usize, 1].into_iter().collect();
    // 切到鼠标工具：`set_tool` 清空选中集之前先把它们快照成刀
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    assert!(editor.mould_walls.is_some(), "音符应被快照成刀");

    // 面团：覆盖空隙的待确认填充矩形（keys 59..63）
    add_pending_rect(&mut editor, 50.0, 350.0, 59);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("面团应有镜像");
    editor.editor_state.shape_select.select_only(id);

    assert_eq!(editor.mould_selected(), Ok(()), "单形状 + 手画音符应能切");
    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.kind, lumino_editor_state::ComposedKind::Mould);
    assert!(composed.results.len() >= 2, "至少外片 + 一个内片");
    let outer = &composed.results[0];
    assert!(
        outer
            .notes
            .iter()
            .all(|n| n.key != 60 || n.tick + n.length <= 100.0 + 1e-3 || n.tick >= 200.0 - 1e-3),
        "外片不应包含空隙内的音符：{:?}",
        outer
            .notes
            .iter()
            .map(|n| (n.key, n.tick, n.length))
            .collect::<Vec<_>>()
    );
    assert!(
        composed.results[1..]
            .iter()
            .flat_map(|part| part.notes.iter())
            .any(|n| n.key == 60 && n.tick >= 100.0 - 1e-3 && n.tick + n.length <= 200.0 + 1e-3),
        "内片应包含空隙内的音符"
    );
    assert!(
        editor.mould_walls.is_none(),
        "刀被消费后应清空（虚线框消失）"
    );
}

#[test]
fn test_mould_knife_preview_only_in_mouse_tool_and_cleared_on_leave() {
    use lumino_message::Tool;

    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[Note::new(0.0, 60, 100.0)]);
    editor.editor_state.interaction.selected_notes = [0_usize].into_iter().collect();
    editor.set_tool(Tool::ShapeSelect);
    assert!(editor.mould_walls.is_some(), "切到鼠标工具应快照刀");
    assert!(
        !editor.overlay_preview().knife_rects.is_empty(),
        "鼠标工具下应显示青色虚线刀框"
    );

    editor.set_tool(Tool::Curve);
    assert!(
        editor.mould_walls.is_none(),
        "离开鼠标工具应清空刀快照，避免虚线框残留"
    );
    assert!(
        editor.overlay_preview().knife_rects.is_empty(),
        "离开鼠标工具后不应再画刀框"
    );

    // 双保险：即使旧快照因撤销/恢复残留，非鼠标工具也不绘制。
    editor.mould_walls = Some(vec![shape_note(0.0, 60)]);
    editor.editor_state.tool = Tool::Curve;
    assert!(
        editor.overlay_preview().knife_rects.is_empty(),
        "非鼠标工具不得绘制刀框"
    );
    editor.editor_state.tool = Tool::ShapeSelect;
    assert!(
        !editor.overlay_preview().knife_rects.is_empty(),
        "鼠标工具下快照恢复后应重新显示"
    );
}
