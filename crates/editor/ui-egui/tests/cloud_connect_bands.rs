//! 云存储连接面板的无头交互测试：表单回传 / 协议 / 连接 / 取消。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, CloudConnectModel, CloudField, CloudFieldModel, CloudProtocol,
    CloudProtocolOptionModel, DialogModel,
};
use lumino_ui_egui::cloud_connect::CloudConnectAction;
use lumino_ui_egui::icons::IconCache;

fn model() -> CloudConnectModel {
    CloudConnectModel {
        titlebar: false,
        title: "连接云存储".to_owned(),
        protocol_label: "协议".to_owned(),
        protocols: [
            ("FTP", CloudProtocol::Ftp, true),
            ("SFTP", CloudProtocol::Sftp, false),
            ("WebDAV", CloudProtocol::Webdav, false),
        ]
        .into_iter()
        .map(|(label, protocol, selected)| CloudProtocolOptionModel {
            protocol,
            label: label.to_owned(),
            selected,
        })
        .collect(),
        fields: vec![
            CloudFieldModel {
                field: CloudField::Name,
                label: "名称（可选）".to_owned(),
                placeholder: "例如：我的 NAS".to_owned(),
                value: "我的".to_owned(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Address,
                label: "服务器地址".to_owned(),
                placeholder: "host.example.com".to_owned(),
                value: "nas.local".to_owned(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Port,
                label: "端口（可选）".to_owned(),
                placeholder: "留空使用默认端口".to_owned(),
                value: String::new(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Username,
                label: "用户名".to_owned(),
                placeholder: "用户名".to_owned(),
                value: "admin".to_owned(),
                password: false,
            },
            CloudFieldModel {
                field: CloudField::Password,
                label: "密码（可选）".to_owned(),
                placeholder: "密码".to_owned(),
                value: "secret".to_owned(),
                password: true,
            },
        ],
        error: None,
        privacy: "Lumino 无权且不会收集您的个人信息，所有信息将会存储在本地".to_owned(),
        connecting: false,
        connect: "连接".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn make_harness(
    model: CloudConnectModel,
) -> (Harness<'static>, Rc<RefCell<Vec<CloudConnectAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(480.0, 500.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::CloudConnect(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::cloud_connect::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn labels_and_buttons_are_visible() {
    let (harness, _) = make_harness(model());
    for label in [
        "连接云存储",
        "协议",
        "名称（可选）",
        "服务器地址",
        "端口（可选）",
        "用户名",
        "密码（可选）",
        "Lumino 无权且不会收集您的个人信息，所有信息将会存储在本地",
        "取消",
        "连接",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

#[test]
fn address_input_dispatches_field_and_text() {
    let (mut harness, actions) = make_harness(model());
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("nas.local")
    });
    input.focus();
    input.type_text("x");
    harness.run();
    let dispatched = actions.borrow().clone();
    assert!(
        dispatched.iter().any(|action| matches!(
            action,
            CloudConnectAction::FieldChanged(CloudField::Address, text) if text.contains('x')
        )),
        "地址输入应回传 FieldChanged(Address)，实际 {dispatched:?}"
    );
}

#[test]
fn connect_and_cancel_dispatch() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("连接").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&CloudConnectAction::Connect));

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&CloudConnectAction::Cancel));
}

/// 协议下拉展开后选择 SFTP，回传协议枚举。
#[test]
fn protocol_combo_dispatches_selection() {
    let (mut harness, actions) = make_harness(model());
    harness
        .get_by(|node| node.role() == egui::Role::ComboBox)
        .click();
    // 弹层在下一帧才展开（egui 的 popup 需要一次额外 pass）。
    harness.run();
    harness.run();
    let sftp = harness
        .get_all_by_label("SFTP")
        .last()
        .expect("下拉里应有 SFTP");
    sftp.click();
    harness.run();
    assert!(
        actions.borrow().iter().any(|action| matches!(
            action,
            CloudConnectAction::ProtocolSelected(CloudProtocol::Sftp)
        )),
        "选择 SFTP 应回传 ProtocolSelected(Sftp)，实际 {:?}",
        actions.borrow()
    );
}

/// 连接中：按钮文案变化，点击不再产生动作。
#[test]
fn connecting_disables_connect() {
    let mut model = model();
    model.connecting = true;
    model.connect = "连接中...".to_owned();
    let (mut harness, actions) = make_harness(model);
    assert!(harness.query_by_label("连接中...").is_some());
    harness.get_by_label("连接中...").click();
    harness.run();
    assert!(
        !actions
            .borrow()
            .iter()
            .any(|action| matches!(action, CloudConnectAction::Connect)),
        "连接中不得再次提交"
    );
}

/// 错误提示可见（危险色文案）。
#[test]
fn error_hint_is_visible() {
    let mut model = model();
    model.error = Some("连接失败：认证被拒绝".to_owned());
    let (harness, _) = make_harness(model);
    assert!(harness.query_by_label("连接失败：认证被拒绝").is_some());
}
