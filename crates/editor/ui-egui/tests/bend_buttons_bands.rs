//! 曲线弯折按钮（egui）：模型几何 / 点击动作 / 占用区。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::bend_buttons::{BendButton, BendButtonsModel, BendKind};
use lumino_ui_egui::chrome::{self, ChromeModel};

fn model() -> BendButtonsModel {
    BendButtonsModel {
        buttons: vec![
            BendButton {
                rect: egui::Rect::from_min_size(egui::pos2(400.0, 300.0), egui::vec2(52.0, 20.0)),
                label: "曲线".to_owned(),
                kind: BendKind::Smooth,
                selected: false,
            },
            BendButton {
                rect: egui::Rect::from_min_size(egui::pos2(456.0, 300.0), egui::vec2(52.0, 20.0)),
                label: "折线".to_owned(),
                kind: BendKind::Corner,
                selected: true,
            },
        ],
    }
}

fn harness(
    model: BendButtonsModel,
) -> (
    Harness<'static>,
    Rc<RefCell<Vec<lumino_editor_state::AnchorBend>>>,
) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::set_bend_buttons(ui.ctx(), model.clone());
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            if let Some(bend) = lumino_ui_egui::bend_buttons::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(bend);
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
fn clicking_each_button_emits_its_bend() {
    for (pos, expected) in [
        (
            egui::pos2(426.0, 310.0),
            lumino_editor_state::AnchorBend::Smooth,
        ),
        (
            egui::pos2(482.0, 310.0),
            lumino_editor_state::AnchorBend::Corner,
        ),
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
        egui::Rect::from_min_size(egui::pos2(400.0, 300.0), egui::vec2(52.0, 20.0)),
        egui::Rect::from_min_size(egui::pos2(456.0, 300.0), egui::vec2(52.0, 20.0)),
    ] {
        assert!(
            blocked.iter().any(|b| b.contains(rect.center())),
            "按钮 {rect:?} 必须占用：{blocked:?}"
        );
    }
}

#[test]
fn empty_model_draws_nothing() {
    let (h, _) = harness(BendButtonsModel::default());
    assert!(chrome::menu::take_blocked(&h.ctx).is_empty());
}
