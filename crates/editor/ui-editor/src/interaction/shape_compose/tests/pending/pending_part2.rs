//! 由 `pending.rs` 拆出的后半段测试，按原顺序保留。

use super::*;

#[test]
fn test_pending_drag_across_tick_zero_keeps_selection_width() {
    use lumino_message::Tool;

    let mut editor = empty_editor();
    editor.editor_state.tool = Tool::ShapeSelect;
    // 4 小节宽（ppq 480 → 4 × 1920 = 7680）
    add_pending_rect(&mut editor, 1920.0, 9600.0, 60);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("待确认矩形应有镜像");
    editor.editor_state.shape_select.select_only(id);

    editor.handle_shape_select_pressed(5000.0, 62.0, false);
    assert!(editor.editor_state.shape_select.is_dragging());
    // 向左拖 1 小节：越过 tick 0，应停在边界，宽度保持 4 小节
    editor.handle_shape_select_moved(3080.0, 62.0);
    let during = editor
        .editor_state
        .shape_select
        .selection_bounds_on(1)
        .expect("拖动中应有选框");
    assert!(
        (during.0 - 0.0).abs() < 1e-3,
        "拖到边界应停在 tick 0：{during:?}"
    );
    assert!(
        (during.1 - during.0 - 7680.0).abs() < 1e-3,
        "选框宽度必须保持 4 小节：{during:?}"
    );

    // 拖回原位：形状与选框一起恢复，不能被压扁
    editor.handle_shape_select_moved(5000.0, 62.0);
    let after = editor
        .editor_state
        .shape_select
        .selection_bounds_on(1)
        .expect("拖回后应有选框");
    assert!(
        (after.0 - 1920.0).abs() < 1e-3,
        "拖回后应回到起点：{after:?}"
    );
    assert!(
        (after.1 - after.0 - 7680.0).abs() < 1e-3,
        "拖回后宽度仍须是 4 小节：{after:?}"
    );
    editor.handle_shape_select_released();
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
    assert!(editor.shape_tools.mould_knife.is_none(), "切完应清刀");
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
    assert!(editor.shape_tools.mould_knife.is_none(), "失效后应清刀");
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

    assert!(
        editor.shape_tools.mould_knife.is_none(),
        "删掉刀音符后应立即清刀"
    );
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
        matches!(
            editor.editor_state.current_tool(),
            Tool::Shape | Tool::ShapeSelect
        ),
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
    assert!(editor.shape_tools.mould_knife.is_some(), "刀应跨工具保留");
    assert!(
        !editor.overlay_preview().knife_rects.is_empty(),
        "鼠标工具下显示刀框"
    );

    editor.set_tool(Tool::Curve);
    assert!(
        editor.shape_tools.mould_knife.is_some(),
        "显式刀不应因切换工具而丢失"
    );
    assert!(
        !editor.overlay_preview().knife_rects.is_empty(),
        "刀是显式会话，任何工具下都应可见"
    );

    // Esc 显式清除刀
    editor.handle_action(lumino_message::EditorAction::EscapePressed);
    assert!(editor.shape_tools.mould_knife.is_none(), "Esc 应清除刀");
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
    assert!(
        editor.shape_tools.mould_knife.is_none(),
        "换工具应收敛失效刀"
    );
}
