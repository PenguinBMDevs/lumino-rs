//! 云文件浏览面板与断连提醒的无头测试。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, CloudBrowserModel, CloudDeviceModel, CloudEntryActionKind, CloudEntryLabels,
    CloudEntryModel, CloudNoticeModel, CloudSavePanelModel, DialogModel,
};
use lumino_ui_egui::cloud_browser::CloudBrowserAction;
use lumino_ui_egui::icons::IconCache;

fn labels() -> CloudEntryLabels {
    CloudEntryLabels {
        download: "下载".to_owned(),
        copy: "复制".to_owned(),
        cut: "剪切".to_owned(),
        rename: "重命名".to_owned(),
        delete: "删除".to_owned(),
    }
}

fn browser_model() -> CloudBrowserModel {
    CloudBrowserModel {
        titlebar: false,
        devices: vec![
            CloudDeviceModel {
                index: 0,
                label: "我的 NAS [ftp] 在线".to_owned(),
                selected: true,
            },
            CloudDeviceModel {
                index: 1,
                label: "备份 [sftp] 离线".to_owned(),
                selected: false,
            },
        ],
        device_placeholder: "选择云存储".to_owned(),
        refresh: "刷新".to_owned(),
        disconnect: "断开".to_owned(),
        paste: "粘贴".to_owned(),
        clipboard: true,
        clear_clipboard: "清空剪贴板".to_owned(),
        back: "← 返回".to_owned(),
        current_path: "/music".to_owned(),
        new_folder_hint: "新建文件夹名称".to_owned(),
        new_folder_input: "New".to_owned(),
        new_folder: "新建文件夹".to_owned(),
        save_panel: None,
        entries: vec![
            CloudEntryModel {
                path: "/music/drums".to_owned(),
                name: "📁 drums".to_owned(),
                is_dir: true,
                size: String::new(),
                renaming: false,
                pending_delete: false,
                actions: vec![CloudEntryActionKind::Cut, CloudEntryActionKind::Rename],
            },
            CloudEntryModel {
                path: "/music/a.lmpj".to_owned(),
                name: "📄 a.lmpj".to_owned(),
                is_dir: false,
                size: "1.2 MB".to_owned(),
                renaming: false,
                pending_delete: false,
                actions: vec![CloudEntryActionKind::Download, CloudEntryActionKind::Copy],
            },
        ],
        placeholder: None,
        notice: Some("下载完成".to_owned()),
        save_mode: false,
        confirm_delete: "确认删除？".to_owned(),
        delete: "删除".to_owned(),
        cancel: "取消".to_owned(),
        rename_hint: "新名称".to_owned(),
        rename_input: String::new(),
        labels: labels(),
        confirm: "确定".to_owned(),
    }
}

fn make_browser(
    model: CloudBrowserModel,
) -> (Harness<'static>, Rc<RefCell<Vec<CloudBrowserAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(720.0, 520.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::CloudBrowser(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::cloud_browser::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn browser_shows_tools_and_entries() {
    let (harness, _) = make_browser(browser_model());
    // 设备下拉以 ComboBox 角色存在（选中文本不作为 label 暴露）。
    assert!(
        harness
            .query_all_by(|node| node.role() == egui::Role::ComboBox)
            .count()
            >= 1,
        "应有设备下拉"
    );
    for label in [
        "刷新",
        "断开",
        "粘贴",
        "清空剪贴板",
        "← 返回",
        "/music",
        "新建文件夹",
        "📄 a.lmpj",
        "1.2 MB",
        "下载完成",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
    // 目录行：行按钮与名称标签各一个节点（同文本）。
    assert!(
        harness.query_all_by_label("📁 drums").count() >= 1,
        "缺少目录行"
    );
}

#[test]
fn tool_buttons_dispatch() {
    let (mut harness, actions) = make_browser(browser_model());
    harness.get_by_label("刷新").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&CloudBrowserAction::Refresh));

    let (mut harness, actions) = make_browser(browser_model());
    harness.get_by_label("断开").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CloudBrowserAction::Disconnect)
    );

    let (mut harness, actions) = make_browser(browser_model());
    harness.get_by_label("← 返回").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&CloudBrowserAction::Back));
}

#[test]
fn dir_row_enters_directory() {
    let (mut harness, actions) = make_browser(browser_model());
    harness
        .get_all_by_label("📁 drums")
        .next()
        .expect("目录行应可定位")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CloudBrowserAction::EnterDir("/music/drums".to_owned()))
    );
}

#[test]
fn file_row_actions_carry_path() {
    let (mut harness, actions) = make_browser(browser_model());
    harness
        .get_all_by_label("下载")
        .next()
        .expect("应有下载按钮")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CloudBrowserAction::Download("/music/a.lmpj".to_owned()))
    );
}

#[test]
fn save_mode_shows_panel_and_save_here() {
    let mut model = browser_model();
    model.save_mode = true;
    model.save_panel = Some(CloudSavePanelModel {
        title: "将保存当前工程到：/music".to_owned(),
        save: "保存到此处".to_owned(),
        busy: false,
    });
    for entry in &mut model.entries {
        entry.actions.clear();
    }
    let (mut harness, actions) = make_browser(model);
    assert!(harness.query_by_label("将保存当前工程到：/music").is_some());
    harness.get_by_label("保存到此处").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&CloudBrowserAction::SaveHere));
}

#[test]
fn pending_delete_shows_confirm_buttons() {
    let mut model = browser_model();
    model.entries[1].pending_delete = true;
    let (mut harness, actions) = make_browser(model);
    assert!(harness.query_by_label("确认删除？").is_some());
    harness
        .get_all_by_label("删除")
        .next()
        .expect("应有删除确认按钮")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CloudBrowserAction::DeleteEntry {
            path: "/music/a.lmpj".to_owned(),
            is_dir: false,
        })
    );
}

#[test]
fn notice_shows_message_and_dismisses() {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let model = ChromeModel {
        dialog: Some(DialogModel::CloudNotice(Box::new(CloudNoticeModel {
            titlebar: false,
            title: "云存储提示".to_owned(),
            message: "连接已断开".to_owned(),
            dismiss: "知道了".to_owned(),
        }))),
        ..ChromeModel::default()
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(440.0, 260.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::cloud_browser::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    assert!(harness.query_by_label("云存储提示").is_some());
    assert!(harness.query_by_label("连接已断开").is_some());
    harness.get_by_label("知道了").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CloudBrowserAction::DismissNotice)
    );
}
