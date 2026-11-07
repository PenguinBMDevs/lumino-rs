//! 溢出菜单的无头交互测试（从 `chrome_bands.rs` 拆出，保持单文件 < 400 行）。

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, OverflowItemModel, OverflowMenuModel, StatusbarModel, ToolbarAction,
    ToolbarModel,
};
use lumino_ui_egui::icons::{Icon, IconCache};

fn statusbar(fps: Option<f32>) -> StatusbarModel {
    StatusbarModel {
        left: "就绪".to_owned(),
        left_accent: fps.is_some(),
        cpu_percent: fps.map(|_| 12.5),
        memory_mb: 256.0,
        fps,
        right: String::new(),
    }
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
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
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

/// 溢出菜单：点「更多」开合；点网格按钮回传下标；禁用项不产生动作。
#[test]
fn overflow_menu_dispatches_items_and_ignores_disabled() {
    let model = std::rc::Rc::new(std::cell::RefCell::new(ChromeModel {
        dialog: None,
        titlebar: None,
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: Some(ToolbarModel {
            overflow_menu: Some(OverflowMenuModel {
                items: vec![
                    OverflowItemModel {
                        icon: Icon::Undo,
                        tooltip: "撤销".to_owned(),
                        enabled: true,
                    },
                    OverflowItemModel {
                        icon: Icon::FlipVertical,
                        tooltip: "垂直翻转".to_owned(),
                        enabled: false,
                    },
                    OverflowItemModel {
                        icon: Icon::Users,
                        tooltip: "协作".to_owned(),
                        enabled: true,
                    },
                ],
            }),
            ..ToolbarModel::default()
        }),
        statusbar: statusbar(None),
    }));
    let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut icons = IconCache::default();
    let model_w = model.clone();
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        // 工具栏固定分组宽：取真实窗口量级，避免「更多」落到视口外。
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model_w.borrow(), &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    let (more, band) =
        chrome::overflow::trigger(&harness.ctx).expect("有溢出时应记录「更多」触发区");
    let panel = chrome::overflow::panel_rect(band, 3);

    // 第一格（可用）→ 下标 0。
    click(
        &mut harness,
        egui::pos2(panel.left() + 8.0 + 20.0, panel.top() + 8.0 + 20.0),
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::OverflowItem(0)),
        "点第一格应回传下标 0"
    );

    // 第二格（禁用）→ 不产生动作。
    let before = actions.borrow().len();
    click(
        &mut harness,
        egui::pos2(
            panel.left() + 8.0 + 20.0,
            panel.top() + 8.0 + 40.0 + 4.0 + 20.0,
        ),
    );
    assert_eq!(actions.borrow().len(), before, "禁用项不得产生溢出动作");

    // 「更多」按钮 → 开合动作。
    click(&mut harness, more.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ToggleOverflowMenu),
        "点更多应请求开合溢出菜单"
    );
}

/// 无溢出时不画「更多」，也不留下会吞点击的触发矩形。
#[test]
fn no_overflow_hides_more_button() {
    let model = ChromeModel {
        dialog: None,
        titlebar: None,
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: Some(ToolbarModel {
            more_visible: false,
            collaboration_visible: true,
            ..ToolbarModel::default()
        }),
        statusbar: statusbar(None),
    };
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons));
    harness.run();
    assert!(
        chrome::overflow::trigger(&harness.ctx).is_none(),
        "无溢出时不得记录「更多」触发区"
    );
}
