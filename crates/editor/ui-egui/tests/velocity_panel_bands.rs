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
        velocity_editing: false,
        velocity_points: Vec::new(),
        has_selection: false,
        selected_notes: Vec::new(),
        view: velocity_panel::VelocityView::default(),
        tempo_editing: false,
        tempo_mode: false,
        tempo_points: Vec::new(),
        tempo_max_bpm: 512.0,
        tempo_line_width: 2.0,
        eraser_tool: false,
        automation_editing: false,
        automation: None,
        value_zoom: 1.0,
        value_scroll: 0.0,
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
fn automation_right_click_deletes_anchor() {
    let mut m = model();
    m.automation_editing = true;
    m.automation = Some(velocity_panel::AutomationLaneModel {
        track_idx: 1,
        lane_idx: Some(0),
        controller: Some(1),
        max_value: 127.0,
        anchors: vec![velocity_panel::AutomationAnchor {
            tick: 1920,
            value: 64,
        }],
    });
    // 锚点：x = 248 + 192 + 120 = 560；y = canvas.top + (h - 64/127*h)
    let h = m.canvas.height();
    let anchor = egui::pos2(560.0, m.canvas.top() + h - (64.0 / 127.0) * h);
    let (mut h, actions) = harness(m);
    click_button(&mut h, anchor, egui::PointerButton::Secondary);
    assert_eq!(
        actions.borrow().last(),
        Some(&velocity_panel::VelocityPanelAction::AutomationDelete {
            track_idx: 1,
            lane_idx: 0,
            tick: 1920,
        }),
        "右键锚点应删除：{:?}",
        actions.borrow()
    );
}

#[test]
fn automation_empty_click_adds_anchor() {
    let mut m = model();
    m.automation_editing = true;
    m.automation = Some(velocity_panel::AutomationLaneModel {
        track_idx: 1,
        lane_idx: None,
        controller: Some(7),
        max_value: 127.0,
        anchors: Vec::new(),
    });
    let (mut h, actions) = harness(m);
    click(&mut h, egui::pos2(700.0, 1100.0));
    match actions.borrow().last() {
        Some(velocity_panel::VelocityPanelAction::AutomationAdd {
            controller: Some(7),
            tick,
            value,
            ..
        }) => {
            assert_eq!(*tick, 1920, "x=700 应吸附到 tick 1920");
            assert!(*value <= 127, "CC 值必须 <= 127：{value}");
        }
        other => panic!("应新增 CC 锚点，实际：{other:?}"),
    }
}

fn click_button(harness: &mut Harness<'static>, pos: egui::Pos2, button: egui::PointerButton) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    input.events.push(egui::Event::PointerButton {
        pos,
        button,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerButton {
        pos,
        button,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

/// 只按下不松开（复现「弹层内按压穿透到画布编辑」）。
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

/// CC 模式的模型：下拉可选，自动化编辑态开启。
fn cc_model() -> VelocityPanelModel {
    let mut m = model();
    m.mode_label = "CC".to_owned();
    m.mode_primary = false;
    m.cc_mode = true;
    m.cc_selected = "1: Modulation Wheel".to_owned();
    m.info = "1: Modulation Wheel".to_owned();
    m
}

/// CC 选择器的点击位置：工具栏左侧模式按钮之后（与文本宽度无关的稳定点）。
const CC_TRIGGER: egui::Pos2 = egui::pos2(400.0, 1014.0);

/// 点击弹层以外的空白处必须关闭 CC 下拉。
#[test]
fn cc_dropdown_closes_on_blank_click() {
    let (mut h, _actions) = harness(cc_model());
    click(&mut h, CC_TRIGGER);
    assert!(
        velocity_panel::cc_menu_open(&h.ctx),
        "点击选择器应展开 CC 下拉"
    );
    // 跑两帧让 `Area` 度过 sizing pass（弹层矩形稳定）。
    h.run();
    h.run();
    assert!(
        velocity_panel::cc_menu_open(&h.ctx),
        "弹层展开后不应自行关闭"
    );

    // 弹层之外、面板数据区以内的空白。
    click(&mut h, egui::pos2(700.0, 1100.0));
    assert!(
        !velocity_panel::cc_menu_open(&h.ctx),
        "点击空白处必须关闭 CC 下拉"
    );
}

/// 弹层（约束上移后盖住画布）内的按压不得穿透到自动化编辑。
#[test]
fn cc_dropdown_press_does_not_reach_automation_canvas() {
    let mut m = cc_model();
    m.automation_editing = true;
    m.automation = Some(velocity_panel::AutomationLaneModel {
        track_idx: 1,
        lane_idx: None,
        controller: Some(1),
        max_value: 127.0,
        anchors: Vec::new(),
    });
    let (mut h, actions) = harness(m);
    click(&mut h, CC_TRIGGER);
    h.run();
    h.run();

    // 弹层内的按压：未松开前不得新增/拖动自动化锚点。
    press(&mut h, egui::pos2(400.0, 1100.0));
    assert!(
        !actions
            .borrow()
            .iter()
            .any(|a| matches!(a, velocity_panel::VelocityPanelAction::AutomationAdd { .. })),
        "弹层内的按压不得穿透到自动化画布：{:?}",
        actions.borrow()
    );
}

/// 菜单展开期间数据区整体惰性：空白处的按压只用于关闭，不顺手改曲线。
#[test]
fn cc_dropdown_blank_press_does_not_edit_canvas() {
    let mut m = cc_model();
    m.automation_editing = true;
    m.automation = Some(velocity_panel::AutomationLaneModel {
        track_idx: 1,
        lane_idx: None,
        controller: Some(1),
        max_value: 127.0,
        anchors: Vec::new(),
    });
    let (mut h, actions) = harness(m);
    click(&mut h, CC_TRIGGER);
    h.run();
    h.run();

    // 弹层之外、面板数据区以内。
    press(&mut h, egui::pos2(700.0, 1100.0));
    assert!(
        !actions.borrow().iter().any(|a| matches!(
            a,
            velocity_panel::VelocityPanelAction::AutomationAdd { .. }
                | velocity_panel::VelocityPanelAction::AutomationMove { .. }
        )),
        "菜单展开期间空白按压不得编辑曲线：{:?}",
        actions.borrow()
    );
}

/// 菜单展开期间数据区进入占用表：宿主不得把空白按压转给下方编辑器。
#[test]
fn cc_dropdown_occupies_canvas_while_open() {
    let (mut h, _actions) = harness(cc_model());
    click(&mut h, CC_TRIGGER);
    h.run();
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(
        blocked
            .iter()
            .any(|rect| rect.contains(egui::pos2(700.0, 1100.0))),
        "菜单展开期间数据区必须进入占用表：{blocked:?}"
    );

    click(&mut h, egui::pos2(700.0, 1100.0));
    h.run();
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(
        !blocked
            .iter()
            .any(|rect| rect.contains(egui::pos2(700.0, 1100.0))),
        "菜单关闭后不得继续占用数据区：{blocked:?}"
    );
}

/// Esc 关闭 CC 下拉（指针停在弹层内也生效）。
#[test]
fn cc_dropdown_closes_on_escape() {
    let (mut h, _actions) = harness(cc_model());
    click(&mut h, CC_TRIGGER);
    h.run();
    h.run();
    assert!(velocity_panel::cc_menu_open(&h.ctx));

    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    });
    *h.input_mut() = input;
    h.run();

    assert!(
        !velocity_panel::cc_menu_open(&h.ctx),
        "Esc 必须关闭 CC 下拉"
    );
}

#[test]
fn resize_handle_drag_changes_panel_height() {
    let m = model();
    let zone = egui::Rect::from_min_size(m.canvas.min, egui::vec2(m.canvas.width(), 5.0));
    let start = zone.center();
    let (mut h, actions) = harness(m);
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(start));
    input.events.push(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    *h.input_mut() = input;
    h.run();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(egui::pos2(
        start.x,
        start.y - 40.0,
    )));
    *h.input_mut() = input;
    h.run();
    match actions.borrow().iter().rev().find_map(|a| match a {
        velocity_panel::VelocityPanelAction::Resize(height) => Some(*height),
        _ => None,
    }) {
        Some(height) => assert!(
            (height - 190.0).abs() < 2.0,
            "上拖 40px 应把 150 高的面板拉到 190，实际 {height}"
        ),
        None => panic!("应回传 Resize，实际：{:?}", actions.borrow()),
    }
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
        "数据区只在编辑态占用：无编辑态时点击应落到画布"
    );
}
