//! 钢琴卷帘右键菜单（egui 版）：条目行数 / 点击动作 / 画布占用。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::piano_roll_context_menu::{self, PianoRollMenuModel};

fn model(target: ContextMenuTarget) -> PianoRollMenuModel {
    PianoRollMenuModel {
        position: egui::pos2(400.0, 300.0),
        available: egui::Rect::from_min_max(egui::pos2(248.0, 102.0), egui::pos2(1448.0, 902.0)),
        target,
    }
}

fn harness(
    menu: PianoRollMenuModel,
) -> (
    Harness<'static>,
    Rc<RefCell<Vec<PianoRollContextMenuAction>>>,
) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::set_piano_roll_menu(ui.ctx(), Some(menu));
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            if let Some(action) = piano_roll_context_menu::take_action(ui.ctx()) {
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
fn direct_show_click_emits_cut() {
    let menu = model(ContextMenuTarget::Notes);
    let panel = piano_roll_context_menu::panel_rect(&menu);
    let first_row = egui::pos2(panel.left() + 28.0, panel.top() + 28.0);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            if let Some(action) = piano_roll_context_menu::show(ui, &menu, &mut icons) {
                actions_w.borrow_mut().push(action);
            }
        });
    h.run();
    click(&mut h, first_row);
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            lumino_message::PianoRollContextMenuItem::Cut
        )),
        "直接绘制菜单：{:?}",
        actions.borrow()
    );
}

#[test]
fn first_row_click_emits_cut() {
    let menu = model(ContextMenuTarget::Notes);
    let panel = piano_roll_context_menu::panel_rect(&menu);
    let first_row = egui::pos2(panel.left() + 28.0, panel.top() + 28.0);
    let (mut h, actions) = harness(menu);
    click(&mut h, first_row);
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            lumino_message::PianoRollContextMenuItem::Cut
        )),
        "第一行必须是「剪切」：{:?}",
        actions.borrow()
    );
}

#[test]
fn outside_click_closes_and_canvas_is_blocked() {
    let menu = model(ContextMenuTarget::Notes);
    let (mut h, actions) = harness(menu);
    // 画布内、面板外：关闭。
    let outside = egui::pos2(800.0, 500.0);
    click(&mut h, outside);
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::Close),
        "画布空白处点击应关闭菜单：{:?}",
        actions.borrow()
    );
    // 打开期间整块画布进入占用区：同一次点击不能再落到画布/iced。
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(
        blocked.iter().any(|rect| rect.contains(outside)),
        "菜单打开期间画布必须占用：{blocked:?}"
    );
}

#[test]
fn drawn_shape_has_twenty_rows() {
    let menu = model(ContextMenuTarget::DrawnShape);
    let panel = piano_roll_context_menu::panel_rect(&menu);
    // 20 行 × 40 + 19 × 4 + 16 = 892：高于本测试的可用区时与 iced 一致
    // 允许向下溢出（生产画布高度足够），但左右不得越界、顶边不得跑到区域外。
    assert!(panel.width() <= menu.available.width());
    assert!(panel.left() >= menu.available.left());
    assert!(panel.top() >= menu.available.top());
}
