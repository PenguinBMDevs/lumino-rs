//! 工具栏精度下拉触发区：点击必须产生 `ToolbarAction::ToggleSnapPicker`。
//!
//! 真实窗口里曾出现「点精度框没反应」：这里用无头 harness 把「几何 + 命中」
//! 固定下来，任何布局/裁剪回归都会先在这里失败。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ChromeModel, StatusbarModel, ToolbarAction, ToolbarModel};
use lumino_ui_egui::icons::IconCache;

fn model() -> ChromeModel {
    ChromeModel {
        titlebar: None,
        toolbar: Some(ToolbarModel {
            // 与生产默认一致：全部条目可见、无溢出。
            visible_tools: usize::MAX,
            ..ToolbarModel::default()
        }),
        statusbar: StatusbarModel {
            left: "就绪".to_owned(),
            left_accent: true,
            cpu_percent: None,
            memory_mb: 0.0,
            fps: None,
            right: String::new(),
        },
        ..ChromeModel::default()
    }
}

fn make_harness() -> (Harness<'static>, Rc<RefCell<Vec<ToolbarAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 120.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model(), &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 一次完整的「移动 → 按下 → 抬起」。
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
fn precision_box_click_toggles_snap_picker() {
    let (mut harness, actions) = make_harness();
    let rect = chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("精度框应发布触发矩形");
    assert!(
        rect.width() > 40.0 && rect.height() > 20.0,
        "精度框尺寸 {rect:?}"
    );

    click(&mut harness, rect.center());

    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ToggleSnapPicker),
        "点击精度框应产生开合动作，实际 {:?}",
        actions.borrow()
    );
}

/// 打开后菜单项可点：点「自定义」产生 `SnapCustom`（打开自定义精度对话框）。
#[test]
fn snap_menu_custom_item_dispatches() {
    let (mut harness, actions) = make_harness();
    let rect = chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("精度框");
    click(&mut harness, rect.center());

    // 模型由生产侧驱动；这里直接给出打开状态，验证面板命中。
    let mut items: Vec<_> = lumino_core::SnapSpec::presets()
        .into_iter()
        .map(|spec| chrome::SnapMenuItem {
            spec,
            label: spec.text(),
            selected: false,
        })
        .collect();
    items.truncate(3);
    let model = ChromeModel {
        toolbar: Some(ToolbarModel {
            visible_tools: usize::MAX,
            snap_menu: Some(chrome::SnapMenuModel {
                items,
                custom_label: "Custom…".to_owned(),
            }),
            ..ToolbarModel::default()
        }),
        ..model()
    };
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 400.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    let rect = chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("精度框");
    let panel = chrome::toolbar_popup::snap_panel_rect(rect, 3, true);
    let row_h = 24.0;
    let custom_y = panel.top() + 6.0 + 3.0 * (row_h + 2.0) + row_h / 2.0;
    click(&mut harness, Pos2::new(panel.left() + 40.0, custom_y));

    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, ToolbarAction::SnapCustom)),
        "点「Custom…」应产生 SnapCustom，实际 {:?}",
        actions.borrow()
    );
}
