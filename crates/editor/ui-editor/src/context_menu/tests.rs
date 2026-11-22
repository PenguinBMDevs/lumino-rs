use super::*;
use lumino_message::PianoRollContextMenuItem;

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
    // 关闭不清 target（保持不可见性，避免下次打开被覆盖）
    state.close();
    assert_eq!(state.target, ContextMenuTarget::DrawnShape);
}

/// 条目集由 `lumino-message` 的 `menu_groups` 单点维护：待确认镜像不得出现 023/024 项。
#[test]
fn test_pending_shape_menu_offers_pending_compose_share_and_delete() {
    // 分组表是真源：常规组 + 待确认专属组，展平后验证专属动作与禁用项。
    let groups = lumino_message::context_menu::menu_groups(ContextMenuTarget::PendingDrawnShape);
    let items: Vec<PianoRollContextMenuItem> =
        groups.iter().flat_map(|g| g.iter().copied()).collect();
    for required in [
        PianoRollContextMenuItem::LiveShape,
        PianoRollContextMenuItem::JoinShapes,
        PianoRollContextMenuItem::MergeShapes,
        PianoRollContextMenuItem::EditShape,
        PianoRollContextMenuItem::PaintAreas,
        PianoRollContextMenuItem::CopyShapeText,
        PianoRollContextMenuItem::ReturnToDrawer,
    ] {
        assert!(items.contains(&required), "待确认镜像缺少条目：{required:?}");
    }
    assert!(
        !items.iter().any(|item| matches!(
            item,
            PianoRollContextMenuItem::Straighten | PianoRollContextMenuItem::StraightenOff
        )),
        "024 扶正没有 pending 目标，不得出现在待确认目标"
    );
}

#[test]
fn test_drawn_shape_menu_is_read_only() {
    let groups = lumino_message::context_menu::menu_groups(ContextMenuTarget::DrawnShape);
    let items: Vec<PianoRollContextMenuItem> =
        groups.iter().flat_map(|g| g.iter().copied()).collect();
    assert_eq!(
        items,
        vec![
            PianoRollContextMenuItem::ShapeList,
            PianoRollContextMenuItem::MaskApply,
            PianoRollContextMenuItem::CopyShapeText,
            PianoRollContextMenuItem::PasteShapeText,
            PianoRollContextMenuItem::DeleteDrawnShape,
        ],
        "DEC-001：已 Bake 图形的菜单只保留只读/删除项"
    );
}
