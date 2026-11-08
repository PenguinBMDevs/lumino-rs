//! 保存确认对话框的无头交互测试：三个按钮 + 标题栏红绿灯 / 拖动。
//!
//! 像素基线由 `dialog_bands` 负责；这里只验证「点击 → SaveConfirmAction」。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, DialogPalette, SaveConfirmModel};
use lumino_ui_egui::dialog::{self, SaveConfirmAction};

fn model(titlebar: bool) -> SaveConfirmModel {
    SaveConfirmModel {
        titlebar,
        palette: DialogPalette::DARK,
        message: "是否保留未保存的更改?".to_owned(),
        save: "保存".to_owned(),
        close: "关闭".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn harness(model: SaveConfirmModel) -> (Harness<'static>, Rc<RefCell<Vec<SaveConfirmAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(560.0, 320.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::SaveConfirm(model.clone())),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = dialog::take_action(ui.ctx()) {
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

fn center(harness: &Harness<'static>, key: &str) -> egui::Pos2 {
    dialog::rect(&harness.ctx, key)
        .unwrap_or_else(|| panic!("缺少命中区 {key}"))
        .center()
}

/// 三个内容按钮：保存 / 关闭（放弃）/ 取消。
#[test]
fn content_buttons_dispatch_actions() {
    let (mut harness, actions) = harness(model(true));
    let pos = center(&harness, "dialog:save");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().last(), Some(&SaveConfirmAction::Save));

    let pos = center(&harness, "dialog:discard");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().last(), Some(&SaveConfirmAction::Discard));

    let pos = center(&harness, "dialog:cancel");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().last(), Some(&SaveConfirmAction::Cancel));
}

/// 标题栏红绿灯：最小化 / 最大化 / 关闭。
#[test]
fn titlebar_buttons_dispatch_actions() {
    let (mut harness, actions) = harness(model(true));
    let pos = center(&harness, "dialog:min");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().last(), Some(&SaveConfirmAction::Minimize));

    let pos = center(&harness, "dialog:max");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&SaveConfirmAction::ToggleMaximize)
    );

    let pos = center(&harness, "dialog:close");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&SaveConfirmAction::CloseWindow)
    );
}

/// 标题栏空白区按下即请求拖动窗口。
#[test]
fn titlebar_drag_area_requests_window_drag() {
    let (mut harness, actions) = harness(model(true));
    let drag = dialog::rect(&harness.ctx, "dialog:drag").expect("拖拽区");
    click(
        &mut harness,
        egui::pos2(drag.left() + 40.0, drag.center().y),
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&SaveConfirmAction::DragWindow)
    );
}

/// 关闭按钮不会误触发拖动（拖拽区不含右侧红绿灯）。
#[test]
fn traffic_buttons_are_outside_drag_area() {
    let (harness, _) = harness(model(true));
    let drag = dialog::rect(&harness.ctx, "dialog:drag").expect("拖拽区");
    let close = dialog::rect(&harness.ctx, "dialog:close").expect("关闭按钮");
    assert!(
        drag.right() <= close.left(),
        "拖拽区右缘 {} 不应覆盖关闭按钮左缘 {}",
        drag.right(),
        close.left()
    );
}

/// 系统原生标题栏模式：不绘制标题栏，也没有标题栏命中区。
#[test]
fn native_titlebar_has_no_traffic_rects() {
    let (harness, _) = harness(model(false));
    assert!(dialog::rect(&harness.ctx, "dialog:drag").is_none());
    assert!(dialog::rect(&harness.ctx, "dialog:close").is_none());
    assert!(dialog::rect(&harness.ctx, "dialog:save").is_some());
}
