use super::*;

#[test]
fn test_state_open_close() {
    let mut state = PianoRollContextMenuState::default();
    assert!(!state.open);
    assert!(state.position.is_none());
    assert_eq!(state.target, ContextMenuTarget::Notes, "默认目标为音符");

    state.open(Point::new(100.0, 200.0), ContextMenuTarget::Notes);
    assert!(state.open);
    assert_eq!(state.position, Some(Point::new(100.0, 200.0)));

    state.close();
    assert!(!state.open);
    assert!(state.position.is_none());
}

#[test]
fn test_state_open_records_target() {
    let mut state = PianoRollContextMenuState::default();
    state.open(Point::new(10.0, 20.0), ContextMenuTarget::DrawnShape);
    assert_eq!(state.target, ContextMenuTarget::DrawnShape);
    // 关闭不清空 target（仅控制可见性），重新打开会覆盖
    state.close();
    assert_eq!(state.target, ContextMenuTarget::DrawnShape);
}

#[test]
fn test_item_icon_mapping() {
    assert_eq!(
        item_icon(PianoRollContextMenuItem::Cut),
        lumino_ui_core::resources::icon::Icon::ContextMenuCut
    );
    assert_eq!(
        item_icon(PianoRollContextMenuItem::Copy),
        lumino_ui_core::resources::icon::Icon::ContextMenuCopy
    );
    assert_eq!(
        item_icon(PianoRollContextMenuItem::Paste),
        lumino_ui_core::resources::icon::Icon::ContextMenuPaste
    );
    assert_eq!(
        item_icon(PianoRollContextMenuItem::Delete),
        lumino_ui_core::resources::icon::Icon::ContextMenuDelete
    );
    assert_eq!(
        item_icon(PianoRollContextMenuItem::SelectAll),
        lumino_ui_core::resources::icon::Icon::ContextMenuSelectAll
    );
    assert_eq!(
        item_icon(PianoRollContextMenuItem::BatchEdit),
        lumino_ui_core::resources::icon::Icon::Gear
    );
    assert_eq!(
        item_icon(PianoRollContextMenuItem::DeleteDrawnShape),
        lumino_ui_core::resources::icon::Icon::ContextMenuDelete
    );
}

#[test]
fn test_pending_shape_menu_offers_pending_compose_share_and_delete() {
    let items = menu_items(ContextMenuTarget::PendingDrawnShape);
    assert_eq!(
        items,
        &[
            PianoRollContextMenuItem::LiveShape,
            PianoRollContextMenuItem::JoinShapes,
            PianoRollContextMenuItem::MergeShapes,
            PianoRollContextMenuItem::EditShape,
            PianoRollContextMenuItem::MaskToggle,
            PianoRollContextMenuItem::MaskSettings,
            PianoRollContextMenuItem::SliceVertical,
            PianoRollContextMenuItem::BetweenCreate,
            PianoRollContextMenuItem::CopyAlong,
            PianoRollContextMenuItem::VelocityUp,
            PianoRollContextMenuItem::CopyShapeText,
            PianoRollContextMenuItem::DeleteDrawnShape,
        ],
        "待确认镜像可走 010/019/020/021/022/025/026/027/028、分享几何、删除"
    );
    assert!(
        !items.iter().any(|item| matches!(
            item,
            PianoRollContextMenuItem::SymmetryVertical
                | PianoRollContextMenuItem::SymmetryHorizontal
                | PianoRollContextMenuItem::Straighten
        )),
        "023/024 仍不得出现在待确认目标"
    );
}

#[test]
fn test_menu_panel_size() {
    let _element = menu_panel(ContextMenuTarget::Notes);
    let _element = menu_panel(ContextMenuTarget::DrawnShape);
    let _element = menu_panel(ContextMenuTarget::PendingDrawnShape);
}

#[test]
fn test_view_returns_element() {
    let _element = view(
        Point::new(50.0, 60.0),
        ContextMenuTarget::Notes,
        Size::new(1280.0, 720.0),
    );
    let _element = view(
        Point::new(50.0, 60.0),
        ContextMenuTarget::DrawnShape,
        Size::new(1280.0, 720.0),
    );
}
