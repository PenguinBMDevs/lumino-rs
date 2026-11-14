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
fn test_pending_mould_requires_selected_knife() {
    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);
    // 没有刀 → 报 NoKnife，而不是笼统的选区错误
    assert_eq!(
        editor.mould_selected(),
        Err(crate::MouldReject::NoKnife),
        "未设刀时应提示先设刀"
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
    // 显式设刀（DEBT-17：不再依赖 set_tool 的顺序）
    let info = editor.mould_set_knife().expect("音符应能设为刀");
    assert_eq!(info.notes, Some(2));
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    assert!(editor.mould_knife.is_some(), "刀应跨工具保留");

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
        !composed.mould_cuts.is_empty(),
        "切口区间必须随组合结果保留（刀清空后仍可画）"
    );
    assert!(
        editor.mould_knife.is_none(),
        "刀被消费后应清空（虚线框消失）"
    );
}

#[test]
fn test_pending_mould_geometry_knife_keeps_hole_as_cut() {
    use lumino_editor_state::{ShapeInstance, ShapeKind};

    let mut editor = empty_editor();
    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    let push_rect = |editor: &mut Editor, rect: (f32, f32, f32, f32)| {
        editor.editor_state.shape_tool.shapes.push(ShapeInstance {
            kind: ShapeKind::Rectangle,
            rect,
            shift_constrained: false,
            filled: true,
            apex_high: true,
            mask: None,
        });
    };
    // 刀：外环 key 59..63 + 内环 key 60..62（嵌套 → 奇偶成孔）
    push_rect(&mut editor, (50.0, 59.0, 350.0, 63.0));
    push_rect(&mut editor, (150.0, 60.0, 250.0, 62.0));
    // 面团：更大的矩形，包住孔
    push_rect(&mut editor, (40.0, 58.0, 360.0, 64.0));
    editor.rebuild_pending_mirrors();

    let knife_ids: Vec<u64> = (0..2)
        .filter_map(|index| {
            editor
                .editor_state
                .shape_select
                .pending_mirror(PendingShapeRef::Shape(index))
        })
        .collect();
    editor.editor_state.shape_select.select_all_of(knife_ids);
    let info = editor.mould_set_knife().expect("形状应能设为几何刀");
    assert_eq!(info.rings, 2, "两环都应进入刀");

    let dough = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(2))
        .expect("面团应有镜像");
    editor.editor_state.shape_select.select_only(dough);

    assert_eq!(editor.mould_selected(), Ok(()), "几何刀应能切出内片");
    let composed = &editor.editor_state.pending_composed[0];
    assert!(composed.results.len() >= 2, "至少外片 + 一个内片");
    assert!(
        composed.mould_cuts.iter().any(|(key, start, end)| {
            *key == 61 && (*start - 150.0).abs() < 1e-3 && (*end - 250.0).abs() < 1e-3
        }),
        "内环应成为切口（孔）：{:?}",
        composed.mould_cuts
    );
    assert!(editor.mould_knife.is_none(), "切完应清刀");
    assert!(
        !editor.overlay_preview().mould_cuts.is_empty(),
        "刀清空后切口仍应可绘制"
    );
}

#[test]
fn test_mould_knife_stale_after_notes_deleted() {
    use lumino_editor_state::ShapeInstance;

    let mut editor = Editor::new();
    seed_notes(
        &mut editor,
        2,
        1,
        &[Note::new(0.0, 60, 100.0), Note::new(200.0, 60, 100.0)],
    );
    editor.editor_state.interaction.selected_notes = [0_usize, 1].into_iter().collect();
    assert!(editor.mould_set_knife().is_ok());
    editor.delete_selected_notes();

    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    editor.editor_state.shape_tool.shapes.push(ShapeInstance {
        kind: lumino_editor_state::ShapeKind::Rectangle,
        rect: (0.0, 59.0, 300.0, 63.0),
        shift_constrained: false,
        filled: true,
        apex_high: true,
        mask: None,
    });
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("面团应有镜像");
    editor.editor_state.shape_select.select_only(id);

    assert_eq!(
        editor.mould_selected(),
        Err(crate::MouldReject::KnifeStale),
        "刀音符被删后应报失效，而不是静默用旧快照"
    );
    assert!(editor.mould_knife.is_none(), "失效后应清刀");
}

#[test]
fn test_delete_knife_notes_emits_expired_notice() {
    let mut editor = Editor::new();
    seed_notes(
        &mut editor,
        2,
        1,
        &[Note::new(0.0, 60, 100.0), Note::new(200.0, 60, 100.0)],
    );
    editor.editor_state.interaction.selected_notes = [0_usize, 1].into_iter().collect();
    assert!(editor.mould_set_knife().is_ok());

    editor.handle_delete_pressed();

    assert!(editor.mould_knife.is_none(), "删掉刀音符后应立即清刀");
    assert_eq!(
        editor.take_mould_notice(),
        Some(crate::MouldNotice::Expired),
        "清刀必须留下一次性通知，由 UI 转 toast"
    );
}

#[test]
fn test_bake_undo_restores_pending_selection_and_tool() {
    use lumino_message::Tool;

    let mut editor = empty_editor();
    setup_single_pending_rect(&mut editor);
    assert!(editor.copy_along_selected(), "先形成一个 pending 组合");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(editor.confirm_shape_tool(), "✓ 写入文档");
    assert!(editor.editor_state.pending_composed.is_empty());

    // 模拟用户 ✓ 后切到无关工具，再撤销
    editor.set_tool(Tool::Pointer);
    assert!(editor.undo(), "撤销应回到 Bake 前的 pending");

    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        matches!(editor.current_tool(), Tool::Shape | Tool::ShapeSelect),
        "撤销回到 pending 时自动回到可确认的工具"
    );
    assert!(
        editor
            .editor_state
            .shape_select
            .selected_shapes()
            .any(|shape| shape.pending == Some(PendingShapeRef::Composed(0))),
        "撤销后组合结果应直接处于选中态，✓× 立即可用"
    );
}

#[test]
fn test_mould_knife_preview_visible_in_every_tool_and_persists() {
    use lumino_message::Tool;

    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[Note::new(0.0, 60, 100.0)]);
    editor.editor_state.interaction.selected_notes = [0_usize].into_iter().collect();
    assert!(editor.mould_set_knife().is_ok());
    assert!(
        !editor.overlay_preview().knife_rects.is_empty(),
        "设刀后立刻可见（不必先切到鼠标工具）"
    );

    editor.set_tool(Tool::ShapeSelect);
    assert!(editor.mould_knife.is_some(), "刀应跨工具保留");
    assert!(
        !editor.overlay_preview().knife_rects.is_empty(),
        "鼠标工具下显示刀框"
    );

    editor.set_tool(Tool::Curve);
    assert!(editor.mould_knife.is_some(), "显式刀不应因切换工具而丢失");
    assert!(
        !editor.overlay_preview().knife_rects.is_empty(),
        "刀是显式会话，任何工具下都应可见"
    );

    // Esc 显式清除刀
    editor.handle_action(lumino_message::EditorAction::EscapePressed);
    assert!(editor.mould_knife.is_none(), "Esc 应清除刀");
    assert!(editor.overlay_preview().knife_rects.is_empty());
}

#[test]
fn test_geometry_knife_preview_disappears_when_source_removed() {
    use lumino_editor_state::{ShapeInstance, ShapeKind};

    let mut editor = empty_editor();
    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
    editor.editor_state.shape_tool.shapes.push(ShapeInstance {
        kind: ShapeKind::Rectangle,
        rect: (0.0, 59.0, 100.0, 63.0),
        shift_constrained: false,
        filled: true,
        apex_high: true,
        mask: None,
    });
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("刀形状应有镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.mould_set_knife().is_ok());
    assert!(!editor.overlay_preview().knife_rings.is_empty());

    // X 掉刀所基于的形状：刀框必须立刻消失，不能留在原地
    editor.editor_state.shape_tool.shapes.clear();
    editor.rebuild_pending_mirrors();
    assert!(
        editor.overlay_preview().knife_rings.is_empty(),
        "源形状被删后刀框应立即消失"
    );

    // 换工具时把失效会话一并收敛
    editor.set_tool(lumino_message::Tool::Pointer);
    assert!(editor.mould_knife.is_none(), "换工具应收敛失效刀");
}
