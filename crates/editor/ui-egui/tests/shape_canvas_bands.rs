//! 形状抽屉画布（egui）无头测试：工具栏点击 / 画布点击与拖动 / 分割线。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::shape_canvas::{
    self, ShapeCanvasAction, ShapeCanvasButton, ShapeCanvasLine, ShapeCanvasModel,
};

fn model() -> ShapeCanvasModel {
    ShapeCanvasModel {
        rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
        bg: egui::Color32::WHITE,
        toolbar_bg: egui::Color32::from_gray(230),
        toolbar_width: 104.0,
        buttons: vec![
            ShapeCanvasButton {
                rect: egui::Rect::from_min_size(egui::pos2(8.0, 16.0), egui::vec2(88.0, 30.0)),
                label: "Pen".to_owned(),
                fill: egui::Color32::from_gray(200),
                text: egui::Color32::BLACK,
                enabled: true,
                index: 0,
            },
            ShapeCanvasButton {
                rect: egui::Rect::from_min_size(egui::pos2(8.0, 250.0), egui::vec2(88.0, 30.0)),
                label: "Undo".to_owned(),
                fill: egui::Color32::from_gray(200),
                text: egui::Color32::BLACK,
                enabled: false,
                index: 6,
            },
        ],
        lines: vec![ShapeCanvasLine {
            from: Pos2::new(120.0, 20.0),
            to: Pos2::new(760.0, 580.0),
            color: egui::Color32::BLACK,
            width: 1.0,
        }],
        ..ShapeCanvasModel::default()
    }
}

fn make_harness() -> (Harness<'static>, Rc<RefCell<Vec<ShapeCanvasAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            shape_canvas::show(ui, &model());
            actions_w
                .borrow_mut()
                .extend(shape_canvas::take_actions(ui.ctx()));
        });
    harness.run();
    (harness, actions)
}

fn click(harness: &mut Harness<'static>, pos: Pos2) {
    let mut move_only = RawInput::default();
    move_only.events.push(Event::PointerMoved(pos));
    *harness.input_mut() = move_only;
    harness.run();

    let mut click = RawInput::default();
    click.events.push(Event::PointerMoved(pos));
    click.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    click.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    *harness.input_mut() = click;
    harness.run();
}

#[test]
fn toolbar_button_dispatches_index() {
    let (mut harness, actions) = make_harness();
    click(&mut harness, Pos2::new(50.0, 30.0));
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, ShapeCanvasAction::Toolbar { index: 0 })),
        "点 Pen 应回传 Toolbar(0)，实际 {:?}",
        actions.borrow()
    );
}

#[test]
fn disabled_toolbar_button_is_not_dispatched() {
    let (mut harness, actions) = make_harness();
    click(&mut harness, Pos2::new(50.0, 265.0));
    assert!(
        !actions
            .borrow()
            .iter()
            .any(|action| matches!(action, ShapeCanvasAction::Toolbar { .. })),
        "置灰按钮不得回传动作"
    );
}

#[test]
fn canvas_click_dispatches_press_and_release() {
    let (mut harness, actions) = make_harness();
    click(&mut harness, Pos2::new(400.0, 300.0));
    let dispatched = actions.borrow().clone();
    assert!(
        dispatched.iter().any(|a| matches!(
            a,
            ShapeCanvasAction::Press { local } if (local.x - 400.0).abs() < 1.0 && (local.y - 300.0).abs() < 1.0
        )),
        "画布点击应回传 Press(400,300)，实际 {dispatched:?}"
    );
    assert!(dispatched.contains(&ShapeCanvasAction::Release));
}

#[test]
fn canvas_drag_dispatches_moved() {
    let (mut harness, actions) = make_harness();
    // 按下 → 移动 → 松开（一次 drag）。
    let start = Pos2::new(300.0, 300.0);
    let end = Pos2::new(420.0, 360.0);
    let mut press = RawInput::default();
    press.events.push(Event::PointerMoved(start));
    press.events.push(Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    *harness.input_mut() = press;
    harness.run();

    let mut drag = RawInput::default();
    drag.events.push(Event::PointerMoved(end));
    *harness.input_mut() = drag;
    harness.run();

    let mut release = RawInput::default();
    release.events.push(Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    *harness.input_mut() = release;
    harness.run();

    let dispatched = actions.borrow().clone();
    assert!(
        dispatched
            .iter()
            .any(|a| matches!(a, ShapeCanvasAction::Moved { .. })),
        "拖动应回传 Moved，实际 {dispatched:?}"
    );
    assert!(dispatched.contains(&ShapeCanvasAction::Release));
}

#[test]
fn splitter_drag_reports_target_width() {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            shape_canvas::show_splitter(
                ui,
                egui::Rect::from_min_size(egui::pos2(250.0, 0.0), egui::vec2(6.0, 600.0)),
                egui::Color32::GRAY,
            );
            actions_w
                .borrow_mut()
                .extend(shape_canvas::take_actions(ui.ctx()));
        });
    harness.run();

    let start = Pos2::new(253.0, 300.0);
    let mut press = RawInput::default();
    press.events.push(Event::PointerMoved(start));
    press.events.push(Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    *harness.input_mut() = press;
    harness.run();

    let mut drag = RawInput::default();
    drag.events
        .push(Event::PointerMoved(Pos2::new(300.0, 300.0)));
    *harness.input_mut() = drag;
    harness.run();

    assert!(
        actions
            .borrow()
            .iter()
            .any(|a| matches!(a, ShapeCanvasAction::Splitter(_))),
        "分割线拖动应回传 Splitter，实际 {:?}",
        actions.borrow()
    );
}
