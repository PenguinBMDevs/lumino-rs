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

/// 条目集由 `lumino-message` 单点维护：待确认镜像不得出现 023/024 项。
#[test]
fn test_pending_shape_menu_offers_pending_compose_share_and_delete() {
    let items = lumino_message::context_menu::menu_items(ContextMenuTarget::PendingDrawnShape);
    assert_eq!(
        items,
        &[
            PianoRollContextMenuItem::LiveShape,
            PianoRollContextMenuItem::JoinShapes,
            PianoRollContextMenuItem::MergeShapes,
            PianoRollContextMenuItem::EditShape,
            PianoRollContextMenuItem::MaskToggle,
            PianoRollContextMenuItem::MaskSettings,
            PianoRollContextMenuItem::SliceCut,
            PianoRollContextMenuItem::MouldShapes,
            PianoRollContextMenuItem::RoundCorners,
            PianoRollContextMenuItem::BetweenCreate,
            PianoRollContextMenuItem::CopyAlong,
            PianoRollContextMenuItem::VelocityUp,
            PianoRollContextMenuItem::CopyShapeText,
            PianoRollContextMenuItem::ReturnToDrawer,
            PianoRollContextMenuItem::DeleteDrawnShape,
        ],
        "待确认镜像可走 010/019/020/021/022/025/026/027/028、分享几何、退回抽屉、删除"
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
