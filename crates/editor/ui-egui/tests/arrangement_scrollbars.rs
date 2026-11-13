//! 走带：滚动条拖拽与「添加音轨」按钮（egui 接管）。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::arrangement::{self, ArrangementAction};
use lumino_ui_egui::chrome::{
    ArrangementModel, ArrangementRulerModel, ArrangementScrollbarsModel, ArrangementTrackListModel,
};

fn model() -> ArrangementModel {
    ArrangementModel {
        ruler: ArrangementRulerModel {
            scroll_x: 0.0,
            zoom_x: 0.1,
            ppq: 960,
            left_inset: 0.0,
            top_inset: 0.0,
            time_signatures: vec![(0, 4, 4)],
            total_ticks: 100_000,
        },
        track_list: ArrangementTrackListModel::default(),
        lane: None,
        scrollbars: Some(ArrangementScrollbarsModel {
            scroll_x: 0.0,
            max_scroll_x: 2000.0,
            zoom_x: 1.0,
            viewport_x: 600.0,
            scroll_y: 0.0,
            max_scroll_y: 800.0,
            zoom_y: 1.0,
            viewport_y: 500.0,
        }),
    }
}

fn harness(model: ArrangementModel) -> (Harness<'static>, Rc<RefCell<Vec<ArrangementAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            if let Some(action) = arrangement::show(ui, ui.max_rect(), &model) {
                actions_w.borrow_mut().push(action);
            }
            if let Some(action) = arrangement::take_action(ui.ctx()) {
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
    *harness.input_mut() = input;
    harness.run();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

fn press(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

fn move_to(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    *harness.input_mut() = input;
    harness.run();
}

fn release(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

/// 底部「+」按钮：按下并松开 → `AddTrack`。
#[test]
fn add_track_button_click_emits_action() {
    let (mut h, actions) = harness(model());
    // 底部行 y∈[552,572]（状态栏之上），按钮 x∈[8,168]。
    click(&mut h, egui::pos2(80.0, 560.0));
    assert!(
        actions.borrow().contains(&ArrangementAction::AddTrack),
        "点击底部 + 应发 AddTrack，实际：{:?}",
        actions.borrow()
    );
}

/// 拖拽水平滚动条滑块：产生 `ScrollX(>0)`。
#[test]
fn drag_horizontal_scrollbar_emits_scroll_x() {
    let (mut h, actions) = harness(model());
    // 轨道 600-4=596；滑块 = 596*600/2000 ≈ 178.8，滑块 x∈[170,349]、y∈[554,570]。
    press(&mut h, egui::pos2(220.0, 560.0));
    move_to(&mut h, egui::pos2(320.0, 560.0));
    let x = actions.borrow().iter().rev().find_map(|a| match a {
        ArrangementAction::ScrollX(x) => Some(*x),
        _ => None,
    });
    let x = x.expect("拖拽应产生 ScrollX");
    assert!(x > 200.0, "右拖 100px 应滚动数百像素，实际 {x}");
    release(&mut h, egui::pos2(320.0, 560.0));
}

/// 拖拽垂直滚动条滑块：产生 `ScrollY(>0)`。
#[test]
fn drag_vertical_scrollbar_emits_scroll_y() {
    let (mut h, actions) = harness(model());
    // 轨道高 516-4=512；滑块 = 512*500/800 = 320，滑块 y∈[38,358]、x∈[774,790]。
    // 取滑块中部（y=150，避开两端 6px 的边缘缩放手势）向下拖 100px。
    press(&mut h, egui::pos2(782.0, 150.0));
    move_to(&mut h, egui::pos2(782.0, 250.0));
    let y = actions.borrow().iter().rev().find_map(|a| match a {
        ArrangementAction::ScrollY(y) => Some(*y),
        _ => None,
    });
    let y = y.expect("拖拽应产生 ScrollY");
    assert!(y > 100.0, "下拖 100px 应滚动数百像素，实际 {y}");
    release(&mut h, egui::pos2(782.0, 250.0));
}
