//! 遮罩参数面板（DRAW-010）：打开、pending 即时写入、已 Bake 草稿 + Apply

use super::*;
use lumino_core::shape::MaskParams;
use lumino_message::MaskPanelAction;
use lumino_ui_editor::MaskEditTarget;

/// 造一个待确认矩形，并选中它的镜像
fn root_with_pending_rect() -> (Root, usize) {
    let mut root = create_root();
    root.editor.editor_state.data.current_track = 1;
    root.editor.set_tool(lumino_message::Tool::ShapeSelect);
    root.editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: lumino_editor_state::ShapeKind::Rectangle,
            rect: (0.0, 60.0, 480.0, 64.0),
            shift_constrained: false,
            filled: true,
            apex_high: true,
            mask: None,
        });
    root.editor.rebuild_pending_mirrors();
    let id = root
        .editor
        .editor_state
        .shape_select
        .pending_mirror(lumino_editor_state::PendingShapeRef::Shape(0))
        .expect("pending 镜像");
    root.editor.editor_state.shape_select.select_only(id);
    (root, 0)
}

#[test]
fn test_open_mask_panel_without_selection_warns() {
    let mut root = create_root();
    root.update(Message::MaskPanel(MaskPanelAction::SetSize(4.0)));
    assert!(!root.mask_panel.open, "未打开时参数动作应被忽略");

    root.open_mask_panel(iced_core::Point::new(10.0, 10.0));
    assert!(!root.mask_panel.open, "无选中目标不应打开面板");
    assert!(!root.toast.is_empty(), "应给出失败提示");
}

#[test]
fn test_pending_mask_panel_writes_through_immediately() {
    let (mut root, index) = root_with_pending_rect();
    root.open_mask_panel(iced_core::Point::new(40.0, 50.0));
    assert!(root.mask_panel.open);
    assert_eq!(
        root.mask_panel.target,
        Some(MaskEditTarget::Pending { index }),
        "应锁定待确认形状"
    );
    assert!(!root.mask_panel.dirty, "pending 没有草稿概念");

    root.update(Message::MaskPanel(MaskPanelAction::SetSize(6.0)));
    let params = root.editor.editor_state.shape_tool.shapes[index]
        .mask
        .clone()
        .expect("参数应即时写入");
    assert!((params.size_keys - 6.0).abs() < 1e-3);

    root.update(Message::MaskPanel(MaskPanelAction::SetInvert(true)));
    assert!(
        root.editor.editor_state.shape_tool.shapes[index]
            .mask
            .as_ref()
            .is_some_and(|m| m.invert)
    );

    root.update(Message::MaskPanel(MaskPanelAction::SetEnabled(false)));
    assert!(
        root.editor.editor_state.shape_tool.shapes[index]
            .mask
            .is_none(),
        "关掉应清除标记"
    );

    root.update(Message::MaskPanel(MaskPanelAction::Close));
    assert!(!root.mask_panel.open);
    assert!(root.mask_panel.target.is_none());
}

#[test]
fn test_baked_mask_panel_needs_apply() {
    let mut root = create_root();
    root.editor.editor_state.data.current_track = 1;
    // 已 Bake：把模型塞进 ShapeStore 并绑定到选中图形
    let mut model =
        lumino_core::shape::ShapeModel::new(0, 1, lumino_core::shape::ShapeKind::Custom, 0.25);
    model.geometry = lumino_core::shape::ShapeGeometry::from_points([
        (0.0, 60.0),
        (480.0, 60.0),
        (480.0, 64.0),
        (0.0, 64.0),
    ]);
    let model_id = root.editor.editor_state.shape_store.insert(model, 0);
    let shape_id = root.editor.editor_state.shape_select.add(
        1,
        None,
        lumino_editor_state::DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)],
        },
        Vec::new(),
    );
    root.editor
        .editor_state
        .shape_select
        .set_model_id(shape_id, Some(model_id));
    root.editor.set_tool(lumino_message::Tool::ShapeSelect);
    root.editor.editor_state.shape_select.select_only(shape_id);

    root.open_mask_panel(iced_core::Point::new(40.0, 50.0));
    assert_eq!(
        root.mask_panel.target,
        Some(MaskEditTarget::Baked { model_id })
    );

    root.update(Message::MaskPanel(MaskPanelAction::SetSize(5.0)));
    assert!(root.mask_panel.dirty, "已 Bake 改参数只标脏");
    assert!(
        root.editor
            .mask_params_of(MaskEditTarget::Baked { model_id })
            .is_none(),
        "未 Apply 前不得写入模型"
    );

    root.update(Message::MaskPanel(MaskPanelAction::Apply));
    assert!(!root.mask_panel.dirty, "Apply 后草稿已清");
    let params = root
        .editor
        .mask_params_of(MaskEditTarget::Baked { model_id })
        .expect("Apply 后应写入 Mask op");
    assert!((params.size_keys - 5.0).abs() < 1e-3);
    assert_eq!(params.own, MaskParams::default().own);
}
