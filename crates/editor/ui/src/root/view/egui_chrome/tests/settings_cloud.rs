//! 云管理页：连接行镜像与 CloudAction 翻译。

use super::*;
use lumino_message::CloudAction;
use lumino_ui_egui::chrome::{DialogModel, SettingsPageModel};
use lumino_ui_egui::settings::SettingsAction;

use crate::Message;

fn settings_root() -> Root {
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let config = lumino_core::storage::config::UiConfig::default();
    Root::new_settings_dialog(&theme, &config)
}

#[test]
fn cloud_page_reflects_connections_and_maps_actions() {
    let mut root = settings_root();
    root.settings.selected_menu_index = 7;
    root.settings.cloud.alert = Some("连接失败".to_owned());
    root.settings.cloud.connections = vec![
        crate::settings::CloudConnItem {
            id: "conn-a".to_owned(),
            name: "WebDAV".to_owned(),
            protocol: "WebDAV".to_owned(),
            address: "https://example.com".to_owned(),
            online: true,
        },
        crate::settings::CloudConnItem {
            id: "conn-b".to_owned(),
            name: "SFTP".to_owned(),
            protocol: "SFTP".to_owned(),
            address: "sftp://example.com".to_owned(),
            online: false,
        },
    ];

    let model = root.egui_chrome_model();
    let Some(DialogModel::Settings(values)) = model.dialog else {
        panic!("云管理页应整窗接管");
    };
    let SettingsPageModel::Cloud(page) = values.page else {
        panic!("第 7 页应为云管理");
    };
    assert!(!page.title.is_empty());
    assert!(
        page.alert
            .as_deref()
            .is_some_and(|text| text.contains("连接失败"))
    );
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0].id, "conn-a");
    assert!(page.rows[0].online);
    assert_eq!(page.rows[0].title, "WebDAV [WebDAV]");
    assert_eq!(page.rows[1].address, "sftp://example.com");
    assert!(!page.add_label.is_empty());
    assert!(!page.empty_text.is_empty());

    // 动作 → Message::Cloud（连接 id 原样带回）。
    assert!(matches!(
        root.settings_action_message(&SettingsAction::CloudAdd),
        Some(Message::Cloud(CloudAction::OpenConnectPanel))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::CloudConnect("conn-b".to_owned())),
        Some(Message::Cloud(CloudAction::ConnectExisting(id))) if id == "conn-b"
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::CloudDisconnect("conn-a".to_owned())),
        Some(Message::Cloud(CloudAction::Disconnect(id))) if id == "conn-a"
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::CloudManage),
        Some(Message::Cloud(CloudAction::OpenBrowserPanel))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::CloudDelete("conn-a".to_owned())),
        Some(Message::Cloud(CloudAction::DeleteConnection(id))) if id == "conn-a"
    ));
}
