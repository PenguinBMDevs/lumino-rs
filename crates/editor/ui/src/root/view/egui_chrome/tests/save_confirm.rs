//! 保存确认对话框：动作翻译单测。

use super::*;
use lumino_ui_egui::dialog::SaveConfirmAction as DialogAction;

use crate::Message;
use crate::message::SaveConfirmAction as ProdAction;

fn root() -> Root {
    Root::new(&lumino_core::storage::config::UiConfig::default())
}

#[test]
fn save_confirm_actions_map_to_messages() {
    let root = root();

    assert!(matches!(
        root.save_confirm_action_message(&DialogAction::Save),
        Some(Message::SaveConfirm(ProdAction::Save))
    ));
    assert!(matches!(
        root.save_confirm_action_message(&DialogAction::Discard),
        Some(Message::SaveConfirm(ProdAction::Discard))
    ));
    assert!(matches!(
        root.save_confirm_action_message(&DialogAction::Cancel),
        Some(Message::SaveConfirm(ProdAction::Cancel))
    ));
    // 窗口关闭按钮等价于取消（不保存、继续编辑）。
    assert!(matches!(
        root.save_confirm_action_message(&DialogAction::CloseWindow),
        Some(Message::SaveConfirm(ProdAction::Cancel))
    ));

    // 标题栏：拖动 / 最小化 / 最大化。
    assert!(matches!(
        root.save_confirm_action_message(&DialogAction::DragWindow),
        Some(Message::Window(crate::window::Event::Drag))
    ));
    assert!(matches!(
        root.save_confirm_action_message(&DialogAction::Minimize),
        Some(Message::Window(crate::window::Event::TrafficAction(
            crate::window::TrafficAction::Minimize
        )))
    ));
    assert!(matches!(
        root.save_confirm_action_message(&DialogAction::ToggleMaximize),
        Some(Message::Window(crate::window::Event::TrafficAction(
            crate::window::TrafficAction::ToggleMaximize
        )))
    ));
}
