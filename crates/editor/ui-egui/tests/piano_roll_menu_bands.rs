//! 钢琴卷帘右键菜单（egui 版）：行列布局 / 点击 / 置灰 / 键盘导航 / 画布占用。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction, PianoRollContextMenuItem};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::piano_roll_context_menu::{self, MenuLayout, PianoRollMenuModel};

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

fn layout_of(harness: &Harness<'static>) -> MenuLayout {
    piano_roll_context_menu::cached_layout(&harness.ctx).expect("菜单应已布局")
}

fn row_center(layout: &MenuLayout, column: usize, row: usize) -> egui::Pos2 {
    piano_roll_context_menu::item_rect(layout, column, row).center()
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

fn press(harness: &mut Harness<'static>, key: egui::Key) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

#[test]
fn direct_show_click_emits_cut() {
    let menu = model(ContextMenuTarget::Notes);
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
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, 0, 0));
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            PianoRollContextMenuItem::Cut
        )),
        "直接绘制菜单：{:?}",
        actions.borrow()
    );
}

#[test]
fn first_row_click_emits_cut() {
    let menu = model(ContextMenuTarget::Notes);
    let (mut h, actions) = harness(menu);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, 0, 0));
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            PianoRollContextMenuItem::Cut
        )),
        "第一列第一行必须是「剪切」：{:?}",
        actions.borrow()
    );
}

#[test]
fn second_column_click_emits_knife() {
    // 音符目标：第二列是设刀 / 取消刀（DEBT-17 从常规列移出）。
    let menu = model(ContextMenuTarget::Notes);
    let (mut h, actions) = harness(menu);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, 1, 0));
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            PianoRollContextMenuItem::MouldSetKnife
        )),
        "第二列第一行应为「以选中内容为刀」：{:?}",
        actions.borrow()
    );
}

#[test]
fn disabled_regular_item_ignores_click() {
    // 待确认图形：常规列的剪切只作用于音符 → 置灰且点击无动作。
    let menu = model(ContextMenuTarget::PendingDrawnShape);
    let (mut h, actions) = harness(menu);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, 0, 0));
    assert!(
        actions.borrow().is_empty(),
        "置灰项不得产生动作：{:?}",
        actions.borrow()
    );
}

#[test]
fn enabled_regular_delete_still_works_for_pending() {
    // 常规列第 4 项「删除」对待确认图形有效（与 DeleteDrawnShape 同一条路径）。
    let menu = model(ContextMenuTarget::PendingDrawnShape);
    let (mut h, actions) = harness(menu);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, 0, 3));
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            PianoRollContextMenuItem::Delete
        )),
        "常规列删除应保持可用：{:?}",
        actions.borrow()
    );
}

#[test]
fn keyboard_moves_across_columns_and_confirms() {
    let menu = model(ContextMenuTarget::Notes);
    let (mut h, actions) = harness(menu);
    // 右 → 第二列（设刀），回车确认。
    press(&mut h, egui::Key::ArrowRight);
    press(&mut h, egui::Key::Enter);
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            PianoRollContextMenuItem::MouldSetKnife
        )),
        "左右方向键应切列、回车触发：{:?}",
        actions.borrow()
    );
}

#[test]
fn keyboard_moves_within_column() {
    let menu = model(ContextMenuTarget::Notes);
    let (mut h, actions) = harness(menu);
    press(&mut h, egui::Key::ArrowDown);
    press(&mut h, egui::Key::ArrowDown);
    press(&mut h, egui::Key::Enter);
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::ItemClicked(
            PianoRollContextMenuItem::Paste
        )),
        "上下方向键应列内移动：{:?}",
        actions.borrow()
    );
}

#[test]
fn keyboard_skips_disabled_item() {
    // 待确认图形：首列第 1 行是置灰的剪切，回车不得产生动作。
    let menu = model(ContextMenuTarget::PendingDrawnShape);
    let (mut h, actions) = harness(menu);
    press(&mut h, egui::Key::Enter);
    assert!(
        actions.borrow().is_empty(),
        "回车落在置灰项上不得触发：{:?}",
        actions.borrow()
    );
}

#[test]
fn hover_between_columns_keeps_menu_open() {
    let menu = model(ContextMenuTarget::Notes);
    let (mut h, actions) = harness(menu);
    let layout = layout_of(&h);
    let first = piano_roll_context_menu::item_rect(&layout, 0, 0);
    let second = piano_roll_context_menu::item_rect(&layout, 1, 0);
    let gap = egui::pos2((first.right() + second.left()) * 0.5, first.center().y);
    for pos in [first.center(), gap, second.center()] {
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::PointerMoved(pos));
        *h.input_mut() = input;
        h.run();
    }
    assert!(
        !actions
            .borrow()
            .contains(&PianoRollContextMenuAction::Close),
        "悬停不得关闭菜单：{:?}",
        actions.borrow()
    );
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(
        blocked.iter().any(|rect| rect.contains(gap)),
        "菜单仍打开时必须继续占用画布：{blocked:?}"
    );
}

#[test]
fn outside_click_closes_and_canvas_is_blocked() {
    let menu = model(ContextMenuTarget::Notes);
    let (mut h, actions) = harness(menu);
    // 画布内、面板外：关闭。
    let outside = egui::pos2(800.0, 850.0);
    click(&mut h, outside);
    assert_eq!(
        actions.borrow().last(),
        Some(&PianoRollContextMenuAction::Close),
        "画布空白处点击应关闭菜单：{:?}",
        actions.borrow()
    );
    // 打开期间整块画布进入占用区：同一次点击不能再落到画布/上一代。
    let blocked = chrome::menu::take_blocked(&h.ctx);
    assert!(
        blocked.iter().any(|rect| rect.contains(outside)),
        "菜单打开期间画布必须占用：{blocked:?}"
    );
}

#[test]
fn drawn_shape_menu_stays_single_column() {
    // DEC-001：已 Bake 图形 5 项只读单列不变。
    let menu = model(ContextMenuTarget::DrawnShape);
    let (h, _actions) = harness(menu);
    let layout = layout_of(&h);
    assert_eq!(layout.column_count(), 1, "单列");
    assert_eq!(layout.panel.height(), 5.0 * 24.0 + 16.0, "五行高");
    // 列宽随最长文字 + 图标 + 内边距变化，只校验下界不再固定为 56。
    assert!(layout.panel.width() >= 16.0 + 16.0 + 8.0);
    assert!(layout.panel.width() <= menu.available.width());
}

#[test]
fn pending_menu_fits_available_height() {
    // 1080p：19 行 = 472px，仍在 500px 与可用区内。
    let menu = model(ContextMenuTarget::PendingDrawnShape);
    let (h, _actions) = harness(menu);
    let layout = layout_of(&h);
    assert_eq!(layout.panel.height(), 19.0 * 24.0 + 16.0);
    assert!(layout.panel.height() <= 500.0);
    assert!(menu.available.contains_rect(layout.panel));
}

#[test]
fn pending_menu_flips_inside_720p_canvas() {
    // 720p：可用区高度 600，靠近底边右键时翻到上方且不越界。
    let mut menu = model(ContextMenuTarget::PendingDrawnShape);
    menu.available = egui::Rect::from_min_max(egui::pos2(248.0, 102.0), egui::pos2(1448.0, 702.0));
    menu.position = egui::pos2(600.0, 690.0);
    let (h, _actions) = harness(menu);
    let layout = layout_of(&h);
    assert!(
        layout.panel.top() >= menu.available.top(),
        "{:?} 越界",
        layout.panel
    );
    assert!(layout.panel.left() >= menu.available.left());
    assert!(layout.panel.bottom() <= menu.available.bottom());
}
