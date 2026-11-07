//! 云管理页交互：添加 / 连接 / 断开 / 管理 / 删除（含禁用态）。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, CloudConnModel, CloudPageModel, DialogModel, SettingsLabels,
    SettingsPageModel, SettingsPalette, SettingsSnapshot,
};
use lumino_ui_egui::settings::{SettingsAction, take_action};

fn labels() -> SettingsLabels {
    SettingsLabels {
        menu: (0..10).map(|index| format!("M{index}")).collect(),
        title: "General".to_owned(),
        eraser_label: "Eraser".to_owned(),
        eraser_options: vec!["Default".to_owned(), "Direct".to_owned()],
        track_add_label: "Track".to_owned(),
        track_add_options: vec!["Auto".to_owned(), "Stay".to_owned()],
        log_section: "Log".to_owned(),
        log_label: "Keep".to_owned(),
        log_placeholder: "10".to_owned(),
        confirm: "OK".to_owned(),
    }
}

fn conn(id: &str, online: bool) -> CloudConnModel {
    CloudConnModel {
        id: id.to_owned(),
        title: format!("{id} [SFTP]"),
        address: format!("sftp://{id}"),
        status: if online { "在线" } else { "离线" }.to_owned(),
        online,
        connect_label: if online { "断开" } else { "连接" }.to_owned(),
        manage_label: "管理文件".to_owned(),
        delete_label: "删除".to_owned(),
    }
}

fn harness(model: ChromeModel) -> (Harness<'static>, Rc<RefCell<Vec<SettingsAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let mut icons = lumino_ui_egui::icons::IconCache::default();
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

fn button(harness: &Harness<'static>, key: &str) -> egui::Pos2 {
    lumino_ui_egui::settings::hits(&harness.ctx)
        .cloud_buttons
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, rect)| rect.center())
        .unwrap_or_else(|| panic!("缺少云管理按钮 {key}"))
}

#[test]
fn cloud_page_dispatches_connection_actions() {
    let page = CloudPageModel {
        title: "云存储管理".to_owned(),
        alert: Some("连接失败".to_owned()),
        add_label: "添加连接".to_owned(),
        empty_text: "尚未添加".to_owned(),
        rows: vec![conn("a", true), conn("b", false)],
    };
    let model = ChromeModel {
        dialog: Some(DialogModel::Settings(Box::new(SettingsSnapshot {
            palette: SettingsPalette::DARK,
            labels: labels(),
            selected: 7,
            eraser: "Default".to_owned(),
            track_add: "Auto".to_owned(),
            log_retention: "10".to_owned(),
            page: SettingsPageModel::Cloud(page),
        }))),
        ..ChromeModel::default()
    };
    let (mut harness, actions) = harness(model);

    let pos = button(&harness, "add");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().last(), Some(&SettingsAction::CloudAdd));

    let pos = button(&harness, "connect:a");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::CloudDisconnect("a".to_owned()))
    );

    let pos = button(&harness, "connect:b");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::CloudConnect("b".to_owned()))
    );

    let pos = button(&harness, "delete:a");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::CloudDelete("a".to_owned()))
    );

    let pos = button(&harness, "manage:a");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().last(), Some(&SettingsAction::CloudManage));

    // 离线连接的「管理文件」不可用：点击不得产生动作。
    let before = actions.borrow().len();
    let pos = button(&harness, "manage:b");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().len(), before, "离线连接的管理按钮必须禁用");
}
