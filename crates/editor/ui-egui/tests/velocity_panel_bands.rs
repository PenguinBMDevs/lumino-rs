//! 力度面板骨架（egui）：模式按钮动作与工具栏占用。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::velocity_panel::{self, VelocityPanelModel};

fn model() -> VelocityPanelModel {
    let panel = egui::Rect::from_min_size(egui::pos2(248.0, 1000.0), egui::vec2(1700.0, 150.0));
    VelocityPanelModel {
        panel,
        toolbar: egui::Rect::from_min_size(panel.min, egui::vec2(panel.width(), 28.0)),
        canvas: egui::Rect::from_min_max(egui::pos2(panel.left(), panel.top() + 28.0), panel.max),
        mode_label: "力度".to_owned(),
        mode_primary: true,
        cc_mode: false,
        cc_selected: String::new(),
        info: "力度 0-127".to_owned(),
        hlines: vec![(10.0, "127".to_owned()), (100.0, "0".to_owned())],
        vlines: vec![(20.0, velocity_panel::LineStrength::Measure)],
    }
}

fn harness(
    model: VelocityPanelModel,
) -> (
    Harness<'static>,
    Rc<RefCell<Vec<velocity_panel::VelocityPanelAction>>>,
) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(2048.0, 1232.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::set_velocity_panel(ui.ctx(), Some(model.clone()));
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            if let Some(action) = velocity_panel::take_action(ui.ctx()) {
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
fn mode_button_toggles() {
    let (mut h, actions) = harness(model());
    click(&mut h, egui::pos2(270.0, 1010.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&velocity_panel::VelocityPanelAction::ToggleMode)
    );
}

#[test]
fn toolbar_is_blocked_but_canvas_is_not() {
    let (h, _) = harness(model());
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(
        blocked
            .iter()
            .any(|b| b.contains(egui::pos2(300.0, 1010.0))),
        "工具栏必须占用：{blocked:?}"
    );
    assert!(
        !blocked
            .iter()
            .any(|b| b.contains(egui::pos2(600.0, 1100.0))),
        "数据区不能占用（iced 仍负责力度编辑）"
    );
}
