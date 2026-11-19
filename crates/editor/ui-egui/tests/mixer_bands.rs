//! 混音台独立窗口的无头交互测试：推子 / 静音 / 主控。
//!
//! 混音台是纯 painter 绘制（没有可访问性文本节点），因此按几何坐标点击，
//! 断言 `mixer::take_actions` 产出的动作。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel};
use lumino_ui_egui::mixer::{
    self, MixerAction, MixerDialogModel, MixerLayout, MixerModel, MixerStrip,
};

const W: f32 = 760.0;
const H: f32 = 248.0;

fn strip(name: &str, master: bool) -> MixerStrip {
    MixerStrip {
        name: name.to_owned(),
        volume: 0.5,
        pan: 0.0,
        muted: false,
        soloed: false,
        meter: 0.25,
        master,
    }
}

fn model() -> MixerModel {
    MixerModel {
        scroll_x: 0.0,
        strips: vec![strip("T0", false), strip("T1", false)],
        master: Some(strip("Master", true)),
    }
}

fn harness() -> (Harness<'static>, Rc<RefCell<Vec<MixerAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let model = model();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(W, H))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::Mixer(Box::new(MixerDialogModel {
                    titlebar: false,
                    mixer: model.clone(),
                }))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            actions_w.borrow_mut().extend(mixer::take_actions(ui.ctx()));
        });
    harness.run();
    (harness, actions)
}

fn press(harness: &mut Harness<'static>, pos: egui::Pos2) {
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
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

/// 点推子中部 → `Gain{index:0}`。
#[test]
fn fader_click_dispatches_gain() {
    let (mut harness, actions) = harness();
    let layout = MixerLayout::dialog(
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(W, H)),
        false,
        &model(),
    );
    let strip = layout.strip_rect(0, 0.0);
    press(
        &mut harness,
        egui::pos2(strip.center().x, strip.top() + 60.0),
    );
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, MixerAction::Gain { index: 0, .. })),
        "推子点击必须产出 Gain(0)：{:?}",
        actions.borrow()
    );
}

/// 点静音格 → `Mute(0)`。
#[test]
fn mute_button_dispatches_toggle() {
    let (mut harness, actions) = harness();
    let layout = MixerLayout::dialog(
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(W, H)),
        false,
        &model(),
    );
    let strip = layout.strip_rect(0, 0.0);
    let third = strip.width() / 3.0;
    press(
        &mut harness,
        egui::pos2(strip.left() + third * 1.5, strip.bottom() - 24.0),
    );
    assert!(
        actions.borrow().contains(&MixerAction::Mute(0)),
        "静音格必须产出 Mute(0)：{:?}",
        actions.borrow()
    );
}

/// 点主控推子 → `MasterVolume`。
#[test]
fn master_fader_dispatches_volume() {
    let (mut harness, actions) = harness();
    let layout = MixerLayout::dialog(
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(W, H)),
        false,
        &model(),
    );
    let master = layout.master.expect("主控条");
    press(&mut harness, master.center());
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, MixerAction::MasterVolume(_))),
        "主控推子必须产出 MasterVolume：{:?}",
        actions.borrow()
    );
}
