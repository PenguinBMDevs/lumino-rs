//! 编辑器悬浮 √× 按钮（egui）：点击动作 / 占用区 / 图标可用。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::editor_action_buttons::{
    self, EditorButton, EditorButtonAction, EditorButtonsModel,
};

fn model() -> EditorButtonsModel {
    let confirm = egui::Rect::from_min_size(egui::pos2(400.0, 300.0), egui::vec2(36.0, 36.0));
    let cancel = egui::Rect::from_min_size(egui::pos2(444.0, 300.0), egui::vec2(36.0, 36.0));
    EditorButtonsModel {
        buttons: vec![
            EditorButton {
                rect: confirm,
                action: EditorButtonAction::ShapeConfirm,
            },
            EditorButton {
                rect: cancel,
                action: EditorButtonAction::ShapeCancel,
            },
        ],
    }
}

fn harness(model: EditorButtonsModel) -> (Harness<'static>, Rc<RefCell<Vec<EditorButtonAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::set_editor_buttons(ui.ctx(), model.clone());
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            if let Some(action) = editor_action_buttons::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

#[test]
fn confirm_and_cancel_emit_actions() {
    for (pos, expected) in [
        (egui::pos2(418.0, 318.0), EditorButtonAction::ShapeConfirm),
        (egui::pos2(462.0, 318.0), EditorButtonAction::ShapeCancel),
    ] {
        let (mut h, actions) = harness(model());
        click(&mut h, pos);
        assert_eq!(
            actions.borrow().last(),
            Some(&expected),
            "点 {pos:?} 应回传 {expected:?}"
        );
    }
}

#[test]
fn buttons_are_blocked_for_iced() {
    let (h, _) = harness(model());
    let blocked = chrome::menu::take_blocked(&h.ctx);
    for rect in [
        egui::Rect::from_min_size(egui::pos2(400.0, 300.0), egui::vec2(36.0, 36.0)),
        egui::Rect::from_min_size(egui::pos2(444.0, 300.0), egui::vec2(36.0, 36.0)),
    ] {
        assert!(
            blocked.iter().any(|b| b.contains(rect.center())),
            "按钮 {rect:?} 必须占用：{blocked:?}"
        );
    }
}

#[test]
fn empty_model_draws_nothing() {
    let (h, _) = harness(EditorButtonsModel::default());
    assert!(chrome::menu::take_blocked(&h.ctx).is_empty());
}
