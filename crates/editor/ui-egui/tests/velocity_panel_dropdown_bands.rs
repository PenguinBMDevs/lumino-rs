//! 力度面板 CC 下拉 × 卷帘滚动条：输入归属的端到端几何。
//!
//! 复现路径：CC 弹层向下展开时被 `Area` 约束上移，正好压住卷帘的水平滚动条。
//! 滚动条用原始指针状态驱动，若不判定图层归属，弹层上的按压会同时拖动下层滚动条。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::piano_roll::PianoRollView;
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::scrollbar;
use lumino_ui_egui::velocity_panel::{self, VelocityPanelModel};

const VIEW: egui::Vec2 = egui::vec2(2048.0, 1232.0);
/// 卷帘左内缩（左侧栏宽度）。
const LEFT: f32 = 248.0;
/// 卷帘上内缩（标题栏 + 工具栏）。
const TOP: f32 = 102.0;
/// 力度面板高度（生产 `VELOCITY_PANEL_HEIGHT`）。
const PANEL_H: f32 = 150.0;
/// 状态栏高度（生产 `layout::STATUSBAR_H`）。
const STATUS_H: f32 = 20.0;
/// 卷帘滚动条宽度（生产 `layout::SCROLLBAR`）。
const SCROLLBAR: f32 = 20.0;

/// 卷帘区域底边 = 力度面板顶边。
fn roll_bottom() -> f32 {
    VIEW.y - STATUS_H - PANEL_H
}

/// 卷帘骨架：只在测试需要的位置上画两条滚动条。
fn piano_roll() -> PianoRollView {
    PianoRollView {
        left_inset: LEFT,
        top_inset: TOP,
        right_inset: 0.0,
        bottom_inset: STATUS_H + PANEL_H,
        ..PianoRollView::default()
    }
}

/// 力度面板：横向与卷帘对齐，纵向紧贴卷帘底部。
fn velocity_model() -> VelocityPanelModel {
    let panel = egui::Rect::from_min_size(
        egui::pos2(LEFT, roll_bottom()),
        egui::vec2(VIEW.x - LEFT, PANEL_H),
    );
    VelocityPanelModel {
        panel,
        toolbar: egui::Rect::from_min_size(panel.min, egui::vec2(panel.width(), 28.0)),
        canvas: egui::Rect::from_min_max(egui::pos2(panel.left(), panel.top() + 28.0), panel.max),
        mode_label: "CC".to_owned(),
        mode_primary: false,
        cc_mode: true,
        cc_selected: "1: Modulation Wheel".to_owned(),
        info: "1: Modulation Wheel".to_owned(),
        hlines: Vec::new(),
        vlines: Vec::new(),
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

fn harness() -> (
    Harness<'static>,
    Rc<RefCell<Vec<scrollbar::DirectedAction>>>,
) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let panel = velocity_model();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(VIEW)
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                piano_roll: Some(piano_roll()),
                ..ChromeModel::default()
            };
            chrome::set_velocity_panel(ui.ctx(), Some(panel.clone()));
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = scrollbar::take_action(ui.ctx()) {
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

/// 弹层压住卷帘水平滚动条时，弹层内的按压不得驱动滚动条。
#[test]
fn dropdown_press_does_not_drag_piano_roll_scrollbar() {
    let (mut h, actions) = harness();
    // 首帧是 `Area` sizing pass：多跑两帧让弹层进入命中集合。
    h.run();
    h.run();

    // 打开 CC 下拉（选择器在工具栏：模式按钮右侧）。
    click(&mut h, egui::pos2(400.0, roll_bottom() + 14.0));
    h.run();
    h.run();
    assert!(
        velocity_panel::cc_menu_open(&h.ctx),
        "CC 下拉应已展开（否则本用例没有覆盖到弹层）"
    );

    // 水平滚动条：卷帘区域底部 SCROLLBAR 那一条。
    let bar_top = roll_bottom() - SCROLLBAR;
    let press_pos = egui::pos2(450.0, bar_top + SCROLLBAR * 0.5);
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(
        blocked
            .iter()
            .any(|rect| rect.height() > 100.0 && rect.contains(press_pos)),
        "约束上移后的弹层必须覆盖水平滚动条（几何前提）：{blocked:?}"
    );

    press(&mut h, press_pos);
    assert!(
        actions.borrow().is_empty(),
        "弹层内的按压不得穿透到卷帘滚动条：{:?}",
        actions.borrow()
    );
}
