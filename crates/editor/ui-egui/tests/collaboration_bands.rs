//! 多人协作对话框的无头测试：四视图内容与动作。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, CollaborationConnectView, CollaborationConnectingView,
    CollaborationInRoomView, CollaborationModel, CollaborationRoomActionsView,
    CollaborationViewModel, DialogModel,
};
use lumino_ui_egui::collaboration::CollaborationAction;
use lumino_ui_egui::icons::IconCache;

fn connect_view() -> CollaborationConnectView {
    CollaborationConnectView {
        host: "localhost".to_owned(),
        host_hint: "服务器地址".to_owned(),
        port: "3000".to_owned(),
        port_hint: "端口".to_owned(),
        username: "用户".to_owned(),
        username_hint: "用户".to_owned(),
        password: String::new(),
        password_hint: "密码".to_owned(),
        invite: String::new(),
        invite_hint: "邀请码（可选）".to_owned(),
        connect: "连接".to_owned(),
        status: String::new(),
    }
}

fn model(view: CollaborationViewModel) -> CollaborationModel {
    CollaborationModel {
        titlebar: false,
        title: "多人协作".to_owned(),
        close: "关闭".to_owned(),
        view,
    }
}

fn make_harness(
    model: CollaborationModel,
) -> (Harness<'static>, Rc<RefCell<Vec<CollaborationAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 480.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::Collaboration(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::collaboration::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn connect_view_shows_all_fields() {
    let (harness, _) = make_harness(model(CollaborationViewModel::Connect(connect_view())));
    for label in ["多人协作", "关闭", "连接"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
    // 输入框内容通过 value 暴露（不是 label）。
    for value in ["localhost", "3000", "用户"] {
        assert!(
            harness
                .query_all_by(|node| node.value().as_deref() == Some(value))
                .count()
                >= 1,
            "缺少输入值：{value}"
        );
    }
}

#[test]
fn connect_button_carries_port() {
    let (mut harness, actions) =
        make_harness(model(CollaborationViewModel::Connect(connect_view())));
    harness.get_by_label("连接").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CollaborationAction::Connect("3000".to_owned()))
    );
}

#[test]
fn connecting_view_shows_status() {
    let mut view = connect_view();
    view.status = String::new();
    let (harness, _) = make_harness(model(CollaborationViewModel::Connecting(
        CollaborationConnectingView {
            status: "正在连接服务器...".to_owned(),
            hint: "请稍候".to_owned(),
        },
    )));
    assert!(harness.query_by_label("正在连接服务器...").is_some());
    assert!(harness.query_by_label("请稍候").is_some());
}

#[test]
fn room_actions_dispatch_create_and_join() {
    let view = CollaborationRoomActionsView {
        room_name: "我的房间".to_owned(),
        room_hint: "房间名称".to_owned(),
        create: "创建房间".to_owned(),
        or: "- 或 -".to_owned(),
        invite: "ABC123".to_owned(),
        invite_hint: "邀请码".to_owned(),
        join: "加入房间".to_owned(),
    };
    let (mut harness, actions) =
        make_harness(model(CollaborationViewModel::RoomActions(view.clone())));
    assert!(harness.query_by_label("- 或 -").is_some());
    harness.get_by_label("创建房间").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CollaborationAction::CreateRoom("我的房间".to_owned()))
    );

    let (mut harness, actions) = make_harness(model(CollaborationViewModel::RoomActions(view)));
    harness.get_by_label("加入房间").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CollaborationAction::JoinRoom("ABC123".to_owned()))
    );
}

#[test]
fn in_room_dispatches_copy_and_disconnect() {
    let view = CollaborationInRoomView {
        room_line: "房间: 我的房间".to_owned(),
        invite_label: "邀请码: ".to_owned(),
        invite_code: "ABC123".to_owned(),
        copy: "复制邀请码".to_owned(),
        disconnect: "断开连接".to_owned(),
    };
    let (mut harness, actions) = make_harness(model(CollaborationViewModel::InRoom(view.clone())));
    harness.get_by_label("复制邀请码").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CollaborationAction::CopyInviteCode)
    );

    let (mut harness, actions) = make_harness(model(CollaborationViewModel::InRoom(view)));
    harness.get_by_label("断开连接").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CollaborationAction::Disconnect)
    );
}

#[test]
fn close_dispatches_from_any_view() {
    let (mut harness, actions) =
        make_harness(model(CollaborationViewModel::Connect(connect_view())));
    harness.get_by_label("关闭").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CollaborationAction::CloseDialog)
    );
}

#[test]
fn host_input_dispatches_text() {
    let (mut harness, actions) =
        make_harness(model(CollaborationViewModel::Connect(connect_view())));
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("localhost")
    });
    input.focus();
    input.type_text("x");
    harness.run();
    assert!(
        actions.borrow().iter().any(|action| matches!(
            action,
            CollaborationAction::HostChanged(text) if text.contains('x')
        )),
        "地址输入应回传 HostChanged，实际 {:?}",
        actions.borrow()
    );
}
