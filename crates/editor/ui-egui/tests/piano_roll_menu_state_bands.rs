//! 钢琴卷帘右键菜单（egui 版）：按编辑状态置灰。
//!
//! 覆盖用户要求的三类状态门控：
//! - 无选中音符 → 剪切 / 复制 / 删除 / 批量编辑不可点；
//! - 无刀源 → 「以选中内容为刀」不可点；
//! - 无刀 → 「取消刀」不可点。
//!
//! 布局与常规交互见 `piano_roll_menu_bands.rs`；本文件只关心状态。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_message::{
    ContextMenuAvailability, ContextMenuTarget, PianoRollContextMenuAction,
    PianoRollContextMenuItem,
};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::piano_roll_context_menu::{self, MenuLayout, PianoRollMenuModel};

fn model(target: ContextMenuTarget, availability: ContextMenuAvailability) -> PianoRollMenuModel {
    PianoRollMenuModel {
        position: egui::pos2(400.0, 300.0),
        available: egui::Rect::from_min_max(egui::pos2(248.0, 102.0), egui::pos2(1448.0, 902.0)),
        target,
        availability,
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

fn row_center(layout: &MenuLayout, row: usize) -> egui::Pos2 {
    piano_roll_context_menu::item_rect(layout, row).center()
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

fn clicked(
    actions: &Rc<RefCell<Vec<PianoRollContextMenuAction>>>,
) -> Vec<PianoRollContextMenuItem> {
    actions
        .borrow()
        .iter()
        .filter_map(|action| match action {
            PianoRollContextMenuAction::ItemClicked(item) => Some(*item),
            _ => None,
        })
        .collect()
}

/// 音符目标的常规组行号：剪切 0、复制 1、粘贴 2、删除 3、全选 4、批量编辑 5。
const CUT_ROW: usize = 0;
const COPY_ROW: usize = 1;
const PASTE_ROW: usize = 2;
const DELETE_ROW: usize = 3;
const BATCH_EDIT_ROW: usize = 5;
/// 音符目标专属组：设刀 8、取消刀 9。
const SET_KNIFE_ROW: usize = 8;
const CLEAR_KNIFE_ROW: usize = 9;

#[test]
fn no_selection_greys_note_actions() {
    let menu = model(ContextMenuTarget::Notes, ContextMenuAvailability::default());
    let (mut h, actions) = harness(menu);
    let layout = layout_of(&h);

    for row in [CUT_ROW, COPY_ROW, DELETE_ROW, BATCH_EDIT_ROW] {
        click(&mut h, row_center(&layout, row));
    }
    assert!(
        clicked(&actions).is_empty(),
        "无选中音符时四项都不得产生动作：{:?}",
        actions.borrow()
    );

    // 粘贴不依赖选区，必须仍然可点（避免把置灰做成“整组禁用”）。
    click(&mut h, row_center(&layout, PASTE_ROW));
    assert_eq!(
        clicked(&actions),
        vec![PianoRollContextMenuItem::Paste],
        "粘贴不依赖音符选区：{:?}",
        actions.borrow()
    );
}

#[test]
fn note_selection_enables_note_actions() {
    let menu = model(
        ContextMenuTarget::Notes,
        ContextMenuAvailability {
            has_note_selection: true,
            has_knife_source: true,
            has_knife: false,
        },
    );
    let (mut h, actions) = harness(menu);
    let layout = layout_of(&h);

    for row in [CUT_ROW, COPY_ROW, DELETE_ROW, BATCH_EDIT_ROW] {
        click(&mut h, row_center(&layout, row));
    }
    assert_eq!(
        clicked(&actions),
        vec![
            PianoRollContextMenuItem::Cut,
            PianoRollContextMenuItem::Copy,
            PianoRollContextMenuItem::Delete,
            PianoRollContextMenuItem::BatchEdit,
        ],
        "有选中音符时四项都应可点：{:?}",
        actions.borrow()
    );
}

#[test]
fn keyboard_enter_does_not_fire_disabled_cut() {
    let menu = model(ContextMenuTarget::Notes, ContextMenuAvailability::default());
    let (mut h, actions) = harness(menu);
    press(&mut h, egui::Key::Enter);
    assert!(
        clicked(&actions).is_empty(),
        "焦点在置灰的剪切上时回车不得触发：{:?}",
        actions.borrow()
    );
}

#[test]
fn set_knife_requires_knife_source() {
    let empty = model(ContextMenuTarget::Notes, ContextMenuAvailability::default());
    let (mut h, actions) = harness(empty);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, SET_KNIFE_ROW));
    assert!(
        clicked(&actions).is_empty(),
        "无音符 / 无待确认形状选中时设刀不得触发：{:?}",
        actions.borrow()
    );

    let source = model(
        ContextMenuTarget::Notes,
        ContextMenuAvailability {
            has_knife_source: true,
            ..ContextMenuAvailability::default()
        },
    );
    let (mut h, actions) = harness(source);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, SET_KNIFE_ROW));
    assert_eq!(
        clicked(&actions),
        vec![PianoRollContextMenuItem::MouldSetKnife],
        "有刀源时设刀应可点：{:?}",
        actions.borrow()
    );
}

#[test]
fn clear_knife_requires_knife() {
    let no_knife = model(
        ContextMenuTarget::Notes,
        ContextMenuAvailability {
            has_knife_source: true,
            has_knife: false,
            ..ContextMenuAvailability::default()
        },
    );
    let (mut h, actions) = harness(no_knife);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, CLEAR_KNIFE_ROW));
    assert!(
        clicked(&actions).is_empty(),
        "没有刀时取消刀不得触发：{:?}",
        actions.borrow()
    );

    let with_knife = model(
        ContextMenuTarget::Notes,
        ContextMenuAvailability {
            has_knife_source: true,
            has_knife: true,
            ..ContextMenuAvailability::default()
        },
    );
    let (mut h, actions) = harness(with_knife);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, CLEAR_KNIFE_ROW));
    assert_eq!(
        clicked(&actions),
        vec![PianoRollContextMenuItem::MouldClearKnife],
        "有刀时取消刀应可点：{:?}",
        actions.borrow()
    );
}

#[test]
fn pending_target_knife_follows_same_state() {
    // 待确认图形菜单：常规 8 行后依次是 LiveShape / Join / Merge / Edit / Paint /
    // MaskToggle / MaskSettings / SymV / SymH / Slice / 设刀（18）/ 取消刀（19）。
    let no_source = model(
        ContextMenuTarget::PendingDrawnShape,
        ContextMenuAvailability::default(),
    );
    let (mut h, actions) = harness(no_source);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, 18));
    click(&mut h, row_center(&layout, 19));
    assert!(
        clicked(&actions).is_empty(),
        "待确认目标的刀条目同样受状态门控：{:?}",
        actions.borrow()
    );

    let source = model(
        ContextMenuTarget::PendingDrawnShape,
        ContextMenuAvailability {
            has_knife_source: true,
            has_knife: true,
            ..ContextMenuAvailability::default()
        },
    );
    let (mut h, actions) = harness(source);
    let layout = layout_of(&h);
    click(&mut h, row_center(&layout, 18));
    click(&mut h, row_center(&layout, 19));
    assert_eq!(
        clicked(&actions),
        vec![
            PianoRollContextMenuItem::MouldSetKnife,
            PianoRollContextMenuItem::MouldClearKnife,
        ],
        "待确认目标有刀源 / 有刀时两项应可点：{:?}",
        actions.borrow()
    );
}
