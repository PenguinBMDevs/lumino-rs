//! 文字工具覆盖层（egui）：输入 / 按钮动作 / 画布占用。

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::text_tool::{TextPreview, TextToolAction, TextToolModel};

fn model(editing: bool) -> TextToolModel {
    let rect = egui::Rect::from_min_size(egui::pos2(400.0, 300.0), egui::vec2(200.0, 120.0));
    TextToolModel {
        rect,
        input_rect: egui::Rect::from_min_size(egui::pos2(400.0, 300.0), egui::vec2(200.0, 28.0)),
        text: String::new(),
        editing,
        placeholder: "输入文字…".to_owned(),
        confirm: Some(egui::Rect::from_min_size(
            egui::pos2(610.0, 338.0),
            egui::vec2(36.0, 36.0),
        )),
        cancel: Some(egui::Rect::from_min_size(
            egui::pos2(654.0, 338.0),
            egui::vec2(36.0, 36.0),
        )),
        mode: Some(egui::Rect::from_min_size(
            egui::pos2(698.0, 338.0),
            egui::vec2(36.0, 36.0),
        )),
        mode_merged: false,
        preview: Some(TextPreview {
            rect,
            width: 2,
            height: 2,
            rgba: Arc::new(vec![255; 16]),
            sig: "2x2:test".to_owned(),
        }),
    }
}

fn harness(model: TextToolModel) -> (Harness<'static>, Rc<RefCell<Vec<TextToolAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::set_text_tool(ui.ctx(), Some(model.clone()));
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            if let Some(action) = lumino_ui_egui::text_tool::take_action(ui.ctx()) {
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
fn buttons_emit_their_actions() {
    for (pos, expected) in [
        (egui::pos2(628.0, 356.0), TextToolAction::Confirm),
        (egui::pos2(672.0, 356.0), TextToolAction::Cancel),
        (egui::pos2(716.0, 356.0), TextToolAction::ModeToggle),
    ] {
        let (mut h, actions) = harness(model(true));
        click(&mut h, pos);
        assert_eq!(
            actions.borrow().last(),
            Some(&expected),
            "点 {pos:?} 应回传 {expected:?}"
        );
    }
}

#[test]
fn typing_emits_text_changed() {
    let (mut h, actions) = harness(model(true));
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Text("A".to_owned()));
    *h.input_mut() = input;
    h.run();
    let got = actions.borrow().last().cloned();
    assert!(
        matches!(got, Some(TextToolAction::TextChanged(ref text)) if text == "A"),
        "输入 A 应回传 TextChanged(\"A\")：{got:?}"
    );
}

#[test]
fn canvas_and_buttons_are_blocked() {
    let (h, _) = harness(model(true));
    let blocked = chrome::menu::take_blocked(&h.ctx);
    for rect in [
        egui::Rect::from_min_size(egui::pos2(400.0, 300.0), egui::vec2(200.0, 120.0)),
        egui::Rect::from_min_size(egui::pos2(610.0, 338.0), egui::vec2(36.0, 36.0)),
        egui::Rect::from_min_size(egui::pos2(698.0, 338.0), egui::vec2(36.0, 36.0)),
    ] {
        assert!(
            blocked.iter().any(|b| b.contains(rect.center())),
            "覆盖层 {rect:?} 必须占用：{blocked:?}"
        );
    }
}

#[test]
fn not_editing_hides_input_but_keeps_box() {
    let (h, _) = harness(model(false));
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(blocked.iter().any(|b| b.contains(egui::pos2(500.0, 360.0))));
}
