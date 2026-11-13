//! 「关于」页 logo 彩蛋飞行层：绘制不 panic、不抢指针。
//!
//! 状态机（位置/旋转/透明度）在 `ui-core` 已有 170 行单测；这里只钉住 egui 侧
//! 的两条硬约束：飞行层**不进入占用区**（不吃任何点击）、无模型时零开销。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ChromeModel, EggModel, menu};

fn harness(egg: Option<EggModel>) -> (Harness<'static>, Rc<RefCell<Vec<()>>>) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let clicks_w = clicks.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            chrome::set_egg(ui.ctx(), egg);
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            let _ = ui.ctx().input(|i| i.pointer.any_click()).then(|| {
                clicks_w.borrow_mut().push(());
            });
        });
    harness.run();
    (harness, clicks)
}

fn flying() -> EggModel {
    EggModel {
        x: 320.0,
        y: 180.0,
        box_size: 96.0,
        icon_size: 64.0,
        angle: 0.8,
        opacity: 0.9,
    }
}

#[test]
fn flying_egg_renders_without_blocking_pointer() {
    let (mut h, _) = harness(Some(flying()));
    // 飞行层不是占用区：点它所在位置仍应落到 旧版 / 下层。
    let blocked = menu::take_blocked(&h.ctx);
    assert!(
        blocked.is_empty(),
        "彩蛋飞行层不得进入占用区（否则会吞掉设置页点击）：{blocked:?}"
    );

    // 在 logo 中心点击：不应 panic，也不应产生任何 egui 侧占用。
    let pos = Pos2::new(320.0 + 48.0, 180.0 + 48.0);
    let mut input = RawInput::default();
    input.events.push(Event::PointerMoved(pos));
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    *h.input_mut() = input;
    h.run();
    assert!(menu::take_blocked(&h.ctx).is_empty());
}

#[test]
fn no_egg_model_is_a_no_op() {
    let (h, _) = harness(None);
    assert!(menu::take_blocked(&h.ctx).is_empty());
}
