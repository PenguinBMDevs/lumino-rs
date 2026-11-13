//! 云存储连接面板：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::{tr, trf};
use lumino_message::CloudProtocolUi;
use lumino_ui_egui::chrome::{
    CloudConnectModel, CloudField, CloudFieldModel, CloudProtocol, CloudProtocolOptionModel,
};

use crate::Message;

impl crate::root::Root {
    /// 云连接面板模型：协议 / 五个输入 / 错误 / 隐私提示 / 按钮。
    pub(super) fn cloud_connect_model(&self) -> CloudConnectModel {
        let state = &self.cloud;
        let protocols = [
            CloudProtocolUi::Ftp,
            CloudProtocolUi::Sftp,
            CloudProtocolUi::Webdav,
        ]
        .into_iter()
        .map(|protocol| CloudProtocolOptionModel {
            protocol: match protocol {
                CloudProtocolUi::Ftp => CloudProtocol::Ftp,
                CloudProtocolUi::Sftp => CloudProtocol::Sftp,
                CloudProtocolUi::Webdav => CloudProtocol::Webdav,
            },
            label: protocol.display_name().to_owned(),
            selected: state.protocol == protocol,
        })
        .collect();

        let fields = vec![
            CloudFieldModel {
                field: CloudField::Name,
                label: tr!("名称（可选）").to_string(),
                placeholder: tr!("例如：我的 NAS").to_string(),
                value: state.name.clone(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Address,
                label: tr!("服务器地址").to_string(),
                placeholder: "host.example.com".to_owned(),
                value: state.address.clone(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Port,
                label: tr!("端口（可选）").to_string(),
                placeholder: tr!("留空使用默认端口").to_string(),
                value: state.port.clone(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Username,
                label: tr!("用户名").to_string(),
                placeholder: tr!("用户名").to_string(),
                value: state.username.clone(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Password,
                label: tr!("密码（可选）").to_string(),
                placeholder: tr!("密码").to_string(),
                value: state.password.clone(),
                password: true,
            },
        ];

        CloudConnectModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("连接云存储").to_string(),
            protocol_label: tr!("协议").to_string(),
            protocols,
            fields,
            error: state
                .connect_error
                .as_deref()
                .map(|e| trf!("连接失败：{e}", e = e)),
            privacy: tr!("Lumino 无权且不会收集您的个人信息，所有信息将会存储在本地").to_string(),
            connecting: state.connecting,
            connect: if state.connecting {
                tr!("连接中...").to_string()
            } else {
                tr!("连接").to_string()
            },
            cancel: tr!("取消").to_string(),
        }
    }

    /// egui 云连接动作 → 生产 `Message`。
    pub(crate) fn cloud_connect_action_message(
        &self,
        action: &lumino_ui_egui::cloud_connect::CloudConnectAction,
    ) -> Option<Message> {
        use lumino_message::CloudAction as Prod;
        use lumino_ui_egui::cloud_connect::CloudConnectAction as Action;
        Some(Message::Cloud(match action {
            Action::ProtocolSelected(protocol) => Prod::ProtocolSelected(match protocol {
                CloudProtocol::Ftp => CloudProtocolUi::Ftp,
                CloudProtocol::Sftp => CloudProtocolUi::Sftp,
                CloudProtocol::Webdav => CloudProtocolUi::Webdav,
            }),
            Action::FieldChanged(field, text) => match field {
                CloudField::Name => Prod::NameChanged(text.clone()),
                CloudField::Address => Prod::AddressChanged(text.clone()),
                CloudField::Port => Prod::PortChanged(text.clone()),
                CloudField::Username => Prod::UsernameChanged(text.clone()),
                CloudField::Password => Prod::PasswordChanged(text.clone()),
            },
            Action::Connect => Prod::Connect,
            Action::Cancel => Prod::ConnectCancel,
        }))
    }
}
