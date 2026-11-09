//! 云浏览 / 断连提醒：模型镜像与动作映射。

use super::*;
use crate::Message;
use lumino_message::CloudAction as Prod;
use lumino_ui_egui::chrome::{CloudEntryActionKind, DialogModel};
use lumino_ui_egui::cloud_browser::CloudBrowserAction as EguiAction;

fn dialog_root(dialog: crate::state::root_state::DialogType) -> Root {
    Root::new_dialog(&lumino_ui_core::Theme::TokyoNightStorm.to_string(), dialog)
}

fn entry(
    path: &str,
    name: &str,
    is_dir: bool,
    size: u64,
) -> crate::state::cloud_state::CloudEntryUi {
    crate::state::cloud_state::CloudEntryUi {
        name: name.to_owned(),
        path: path.to_owned(),
        is_dir,
        size,
        modified: None,
    }
}

#[test]
fn browser_model_mirrors_state_and_filters() {
    let mut root = dialog_root(crate::state::root_state::DialogType::CloudBrowser);
    root.cloud.connections = vec![crate::state::cloud_state::CloudConnInfo {
        id: "c1".to_owned(),
        name: "我的 NAS".to_owned(),
        protocol: "ftp".to_owned(),
        online: true,
    }];
    root.cloud.selected_id = Some("c1".to_owned());
    root.cloud.current_path = "/music".to_owned();
    root.cloud.entries = vec![
        entry("/music/drums", "drums", true, 0),
        entry("/music/a.lmpj", "a.lmpj", false, 1024),
    ];
    root.cloud.new_folder_input = "New".to_owned();
    root.cloud.notice = Some("下载完成".to_owned());

    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudBrowser(panel)) = model.dialog else {
        panic!("云浏览应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.devices.len(), 1);
    assert!(panel.devices[0].selected);
    assert!(panel.devices[0].label.contains("我的 NAS"));
    assert_eq!(panel.current_path, "/music");
    assert_eq!(panel.new_folder_input, "New");
    assert_eq!(panel.entries.len(), 2);
    assert!(panel.entries[0].is_dir);
    assert!(panel.entries[0].name.starts_with('📁'));
    assert_eq!(
        panel.entries[0].actions,
        vec![
            CloudEntryActionKind::Cut,
            CloudEntryActionKind::Rename,
            CloudEntryActionKind::Delete
        ]
    );
    assert!(panel.entries[1].name.starts_with('📄'));
    assert_eq!(
        panel.entries[1].actions,
        vec![
            CloudEntryActionKind::Download,
            CloudEntryActionKind::Copy,
            CloudEntryActionKind::Cut,
            CloudEntryActionKind::Rename,
            CloudEntryActionKind::Delete
        ]
    );
    assert!(
        panel.entries[1].size.ends_with("KB"),
        "{}",
        panel.entries[1].size
    );
    assert_eq!(panel.notice.as_deref(), Some("下载完成"));

    // 扩展名过滤：只保留目录与 .lmpj。
    root.cloud.filter = Some("lmpj".to_owned());
    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudBrowser(panel)) = model.dialog else {
        panic!("云浏览应由 egui 整窗接管");
    };
    assert_eq!(panel.entries.len(), 2, "目录始终保留");
    root.cloud.filter = Some("wav".to_owned());
    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudBrowser(panel)) = model.dialog else {
        panic!("云浏览应由 egui 整窗接管");
    };
    assert_eq!(panel.entries.len(), 1, "只剩目录");
}

#[test]
fn browser_empty_and_save_mode_states() {
    let mut root = dialog_root(crate::state::root_state::DialogType::CloudBrowser);
    root.cloud.save_mode = true;
    root.cloud.busy = true;
    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudBrowser(panel)) = model.dialog else {
        panic!("云浏览应由 egui 整窗接管");
    };
    assert!(panel.save_mode);
    let save = panel.save_panel.expect("save_mode 时应带上目标区");
    assert!(save.title.contains('/'));
    assert_eq!(save.save, "上传中...");
    assert_eq!(
        panel.placeholder.as_deref(),
        Some("加载中..."),
        "busy 且无条目应显示加载中"
    );

    root.cloud.busy = false;
    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudBrowser(panel)) = model.dialog else {
        panic!("云浏览应由 egui 整窗接管");
    };
    assert_eq!(panel.placeholder.as_deref(), Some("（空目录）"));
}

#[test]
fn notice_model_mirrors_alert() {
    let mut root = dialog_root(crate::state::root_state::DialogType::CloudNotice);
    root.cloud.alert_message = Some("连接已断开".to_owned());
    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudNotice(panel)) = model.dialog else {
        panic!("提醒应由 egui 整窗接管");
    };
    assert_eq!(panel.title, "云存储提示");
    assert_eq!(panel.message, "连接已断开");
    assert_eq!(panel.dismiss, "知道了");

    // 无 alert 时回落到默认文案。
    root.cloud.alert_message = None;
    let model = root.egui_chrome_model();
    let Some(DialogModel::CloudNotice(panel)) = model.dialog else {
        panic!("提醒应由 egui 整窗接管");
    };
    assert_eq!(panel.message, "云存储连接异常");
}

#[test]
fn browser_actions_map_to_cloud_messages() {
    let mut root = dialog_root(crate::state::root_state::DialogType::CloudBrowser);
    root.cloud.connections = vec![crate::state::cloud_state::CloudConnInfo {
        id: "c1".to_owned(),
        name: "NAS".to_owned(),
        protocol: "ftp".to_owned(),
        online: true,
    }];
    root.cloud.selected_id = Some("c1".to_owned());
    root.cloud.new_folder_input = "New".to_owned();

    assert!(matches!(
        root.cloud_browser_action_message(&EguiAction::SelectDevice(0)),
        Some(Message::Cloud(Prod::SelectStorage(id))) if id == "c1"
    ));
    assert!(
        root.cloud_browser_action_message(&EguiAction::SelectDevice(3))
            .is_none(),
        "越界下标不得产生消息"
    );
    assert!(matches!(
        root.cloud_browser_action_message(&EguiAction::Refresh),
        Some(Message::Cloud(Prod::Refresh))
    ));
    assert!(matches!(
        root.cloud_browser_action_message(&EguiAction::Disconnect),
        Some(Message::Cloud(Prod::Disconnect(id))) if id == "c1"
    ));
    assert!(matches!(
        root.cloud_browser_action_message(&EguiAction::Back),
        Some(Message::Cloud(Prod::Back))
    ));
    assert!(matches!(
        root.cloud_browser_action_message(&EguiAction::NewFolder),
        Some(Message::Cloud(Prod::NewFolder(name))) if name == "New"
    ));
    match root
        .cloud_browser_action_message(&EguiAction::DeleteEntry {
            path: "/a".to_owned(),
            is_dir: true,
        })
        .expect("删除应映射")
    {
        Message::Cloud(Prod::DeleteEntry { path, is_dir }) => {
            assert_eq!(path, "/a");
            assert!(is_dir);
        }
        other => panic!("删除映射错误：{other:?}"),
    }
    assert!(matches!(
        root.cloud_browser_action_message(&EguiAction::DismissNotice),
        Some(Message::Cloud(Prod::DismissAlert))
    ));
}
