//! 多人协作：四视图模型镜像与动作映射。

use super::*;
use crate::Message;
use lumino_ui_egui::chrome::{CollaborationViewModel, DialogModel};
use lumino_ui_egui::collaboration::CollaborationAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::Collaboration,
    )
}

#[test]
fn connect_view_mirrors_state() {
    let mut root = dialog_root();
    root.state.collaboration_dialog.server_host = "10.0.0.2".to_owned();
    root.state.collaboration_dialog.server_port = "4000".to_owned();
    root.state.collaboration_dialog.username = "alice".to_owned();
    root.state.collaboration_dialog.password = "pw".to_owned();
    root.state.collaboration_dialog.invite_code = "INV".to_owned();
    root.state.collaboration_dialog.connection_status = "已断开".to_owned();

    let model = root.egui_chrome_model();
    let Some(DialogModel::Collaboration(panel)) = model.dialog else {
        panic!("协作应由 egui 整窗接管");
    };
    assert!(panel.titlebar);
    assert_eq!(panel.title, "多人协作");
    assert_eq!(panel.close, "关闭");
    let CollaborationViewModel::Connect(view) = &panel.view else {
        panic!("默认视图应为连接");
    };
    assert_eq!(view.host, "10.0.0.2");
    assert_eq!(view.port, "4000");
    assert_eq!(view.username, "alice");
    assert_eq!(view.password, "pw");
    assert_eq!(view.invite, "INV");
    assert_eq!(view.connect, "连接");
    assert_eq!(view.status, "已断开");
}

#[test]
fn connecting_and_in_room_views_mirror_state() {
    let mut root = dialog_root();
    root.state.collaboration_dialog.view_state =
        crate::state::root_state::CollaborationViewState::Connecting;
    let model = root.egui_chrome_model();
    let Some(DialogModel::Collaboration(panel)) = model.dialog else {
        panic!("协作应由 egui 整窗接管");
    };
    let CollaborationViewModel::Connecting(view) = &panel.view else {
        panic!("应为连接中视图");
    };
    assert!(!view.status.is_empty());
    assert_eq!(view.hint, "请稍候");

    root.state.collaboration_dialog.view_state =
        crate::state::root_state::CollaborationViewState::InRoom;
    root.state.collaboration_dialog.room_name = "合奏".to_owned();
    root.state.collaboration_dialog.invite_code = "ABC".to_owned();
    let model = root.egui_chrome_model();
    let Some(DialogModel::Collaboration(panel)) = model.dialog else {
        panic!("协作应由 egui 整窗接管");
    };
    let CollaborationViewModel::InRoom(view) = &panel.view else {
        panic!("应为房间内视图");
    };
    assert_eq!(view.room_line, "房间: 合奏");
    assert_eq!(view.invite_code, "ABC");
    assert_eq!(view.copy, "复制邀请码");
    assert_eq!(view.disconnect, "断开连接");
}

#[test]
fn room_actions_view_mirrors_state() {
    let mut root = dialog_root();
    root.state.collaboration_dialog.view_state =
        crate::state::root_state::CollaborationViewState::RoomActions;
    root.state.collaboration_dialog.room_name = "排练".to_owned();
    let model = root.egui_chrome_model();
    let Some(DialogModel::Collaboration(panel)) = model.dialog else {
        panic!("协作应由 egui 整窗接管");
    };
    let CollaborationViewModel::RoomActions(view) = &panel.view else {
        panic!("应为房间操作视图");
    };
    assert_eq!(view.room_name, "排练");
    assert_eq!(view.create, "创建房间");
    assert_eq!(view.or, "- 或 -");
    assert_eq!(view.join, "加入房间");
}

#[test]
fn collaboration_actions_map_to_messages() {
    use lumino_message::CollaborationAction as Prod;
    let mut root = dialog_root();
    root.state.collaboration_dialog.server_host = "10.0.0.2".to_owned();
    root.state.collaboration_dialog.username = "alice".to_owned();

    match root
        .collaboration_action_message(&EguiAction::Connect("4000".to_owned()))
        .expect("连接应映射")
    {
        Message::Collaboration(Prod::Connect {
            host,
            port,
            username,
            invite_code,
            ..
        }) => {
            assert_eq!(host, "10.0.0.2");
            assert_eq!(port, 4000);
            assert_eq!(username, "alice");
            assert!(invite_code.is_none(), "空邀请码应为 None");
        }
        other => panic!("连接映射错误：{other:?}"),
    }

    // 非法端口回落到生产默认端口。
    match root
        .collaboration_action_message(&EguiAction::Connect("abc".to_owned()))
        .expect("连接应映射")
    {
        Message::Collaboration(Prod::Connect { port, .. }) => {
            assert_eq!(port, crate::constants::collaboration::DEFAULT_PORT)
        }
        other => panic!("连接映射错误：{other:?}"),
    }

    assert!(matches!(
        root.collaboration_action_message(&EguiAction::HostChanged("h".to_owned())),
        Some(Message::Collaboration(Prod::HostChanged(value))) if value == "h"
    ));
    assert!(matches!(
        root.collaboration_action_message(&EguiAction::CreateRoom("r".to_owned())),
        Some(Message::Collaboration(Prod::CreateRoom { name })) if name == "r"
    ));
    assert!(matches!(
        root.collaboration_action_message(&EguiAction::JoinRoom("c".to_owned())),
        Some(Message::Collaboration(Prod::JoinRoom { invite_code })) if invite_code == "c"
    ));
    assert!(matches!(
        root.collaboration_action_message(&EguiAction::CopyInviteCode),
        Some(Message::Collaboration(Prod::CopyInviteCode))
    ));
    assert!(matches!(
        root.collaboration_action_message(&EguiAction::Disconnect),
        Some(Message::Collaboration(Prod::Disconnect))
    ));
    assert!(matches!(
        root.collaboration_action_message(&EguiAction::CloseDialog),
        Some(Message::Collaboration(Prod::CloseDialog))
    ));
}
