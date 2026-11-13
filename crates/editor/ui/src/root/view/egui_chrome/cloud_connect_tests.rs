//! 云连接面板：模型镜像与动作映射。

use super::*;
use crate::Message;
use lumino_message::CloudAction as Prod;
use lumino_ui_egui::chrome::{CloudField, CloudProtocol, DialogModel};
use lumino_ui_egui::cloud_connect::CloudConnectAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::CloudConnect,
    )
}

#[test]
fn cloud_connect_model_mirrors_state() {
    let mut root = dialog_root();
    root.cloud.name = "我的 NAS".to_owned();
    root.cloud.address = "nas.local".to_owned();
    root.cloud.port = "2121".to_owned();
    root.cloud.username = "admin".to_owned();
    root.cloud.password = "secret".to_owned();
    root.cloud.connect_error = Some("认证被拒绝".to_owned());
    root.cloud.connecting = true;

    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudConnect(panel)) = model.dialog else {
        panic!("云连接面板应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.title, "连接云存储");
    assert_eq!(panel.protocol_label, "协议");
    assert_eq!(panel.protocols.len(), 3);
    assert!(panel.protocols[0].selected, "默认协议 FTP 应高亮");
    assert_eq!(panel.protocols[1].label, "SFTP");
    assert_eq!(panel.protocols[2].protocol, CloudProtocol::Webdav);

    assert_eq!(panel.fields.len(), 5, "五个输入行常显");
    assert_eq!(panel.fields[0].field, CloudField::Name);
    assert_eq!(panel.fields[0].label, "名称（可选）");
    assert_eq!(panel.fields[0].value, "我的 NAS");
    assert_eq!(panel.fields[1].field, CloudField::Address);
    assert_eq!(panel.fields[1].value, "nas.local");
    assert_eq!(panel.fields[2].value, "2121");
    assert_eq!(panel.fields[3].value, "admin");
    assert_eq!(panel.fields[4].field, CloudField::Password);
    assert!(panel.fields[4].password, "密码框应掩码显示");
    assert_eq!(panel.fields[4].value, "secret");

    assert_eq!(panel.error.as_deref(), Some("连接失败：认证被拒绝"));
    assert!(panel.privacy.contains("本地"));
    assert!(panel.connecting);
    assert_eq!(panel.connect, "连接中...");
    assert_eq!(panel.cancel, "取消");
}

#[test]
fn cloud_connect_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.cloud_connect_action_message(&EguiAction::ProtocolSelected(CloudProtocol::Sftp)),
        Some(Message::Cloud(Prod::ProtocolSelected(
            lumino_message::CloudProtocolUi::Sftp
        )))
    ));
    assert!(matches!(
        root.cloud_connect_action_message(&EguiAction::FieldChanged(
            CloudField::Address,
            "nas.local".to_owned()
        )),
        Some(Message::Cloud(Prod::AddressChanged(value))) if value == "nas.local"
    ));
    assert!(matches!(
        root.cloud_connect_action_message(&EguiAction::FieldChanged(
            CloudField::Password,
            "pw".to_owned()
        )),
        Some(Message::Cloud(Prod::PasswordChanged(value))) if value == "pw"
    ));
    assert!(matches!(
        root.cloud_connect_action_message(&EguiAction::Connect),
        Some(Message::Cloud(Prod::Connect))
    ));
    assert!(matches!(
        root.cloud_connect_action_message(&EguiAction::Cancel),
        Some(Message::Cloud(Prod::ConnectCancel))
    ));
}
