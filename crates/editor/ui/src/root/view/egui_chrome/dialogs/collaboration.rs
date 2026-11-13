//! 多人协作：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::{tr, trf};
use lumino_ui_egui::chrome::{
    CollaborationConnectView, CollaborationConnectingView, CollaborationInRoomView,
    CollaborationModel, CollaborationRoomActionsView, CollaborationViewModel,
};

use crate::Message;

impl crate::root::Root {
    /// 协作对话框模型：四视图按 `view_state` 选择。
    pub(super) fn collaboration_model(&self) -> CollaborationModel {
        let state = &self.state.collaboration_dialog;
        let view = match state.view_state {
            crate::state::root_state::CollaborationViewState::Connect => {
                CollaborationViewModel::Connect(CollaborationConnectView {
                    host: state.server_host.clone(),
                    host_hint: tr!("服务器地址").to_string(),
                    port: state.server_port.clone(),
                    port_hint: tr!("端口").to_string(),
                    username: state.username.clone(),
                    username_hint: tr!("用户").to_string(),
                    password: state.password.clone(),
                    password_hint: tr!("密码").to_string(),
                    invite: state.invite_code.clone(),
                    invite_hint: tr!("邀请码（可选）").to_string(),
                    connect: tr!("连接").to_string(),
                    status: state.connection_status.clone(),
                })
            }
            crate::state::root_state::CollaborationViewState::Connecting => {
                CollaborationViewModel::Connecting(CollaborationConnectingView {
                    status: tr!("正在连接服务器...").to_string(),
                    hint: tr!("请稍候").to_string(),
                })
            }
            crate::state::root_state::CollaborationViewState::RoomActions => {
                CollaborationViewModel::RoomActions(CollaborationRoomActionsView {
                    room_name: state.room_name.clone(),
                    room_hint: tr!("房间名称").to_string(),
                    create: tr!("创建房间").to_string(),
                    or: tr!("- 或 -").to_string(),
                    invite: state.invite_code.clone(),
                    invite_hint: tr!("邀请码").to_string(),
                    join: tr!("加入房间").to_string(),
                })
            }
            crate::state::root_state::CollaborationViewState::InRoom => {
                CollaborationViewModel::InRoom(CollaborationInRoomView {
                    room_line: trf!("房间: {name}", name = state.room_name),
                    invite_label: tr!("邀请码: ").to_string(),
                    invite_code: state.invite_code.clone(),
                    copy: tr!("复制邀请码").to_string(),
                    disconnect: tr!("断开连接").to_string(),
                })
            }
        };

        CollaborationModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("多人协作").to_string(),
            close: tr!("关闭").to_string(),
            view,
        }
    }

    /// egui 协作动作 → 生产 `Message`。
    pub(crate) fn collaboration_action_message(
        &self,
        action: &lumino_ui_egui::collaboration::CollaborationAction,
    ) -> Option<Message> {
        use lumino_message::CollaborationAction as Prod;
        use lumino_ui_egui::collaboration::CollaborationAction as Action;
        Some(Message::Collaboration(match action {
            Action::HostChanged(text) => Prod::HostChanged(text.clone()),
            Action::PortChanged(text) => Prod::PortChanged(text.clone()),
            Action::UsernameChanged(text) => Prod::UsernameChanged(text.clone()),
            Action::PasswordChanged(text) => Prod::PasswordChanged(text.clone()),
            Action::InviteCodeChanged(text) => Prod::InviteCodeChanged(text.clone()),
            Action::RoomNameChanged(text) => Prod::RoomNameChanged(text.clone()),
            Action::Connect(port) => {
                let state = &self.state.collaboration_dialog;
                Prod::Connect {
                    host: state.server_host.clone(),
                    port: port
                        .parse()
                        .unwrap_or(crate::constants::collaboration::DEFAULT_PORT),
                    username: state.username.clone(),
                    password: state.password.clone(),
                    invite_code: if state.invite_code.trim().is_empty() {
                        None
                    } else {
                        Some(state.invite_code.clone())
                    },
                }
            }
            Action::CreateRoom(name) => Prod::CreateRoom { name: name.clone() },
            Action::JoinRoom(code) => Prod::JoinRoom {
                invite_code: code.clone(),
            },
            Action::CopyInviteCode => Prod::CopyInviteCode,
            Action::Disconnect => Prod::Disconnect,
            Action::CloseDialog => Prod::CloseDialog,
        }))
    }
}
