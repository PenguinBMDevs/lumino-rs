//! 右侧栏「素材库」面板的无头交互测试。
//!
//! 覆盖添加菜单、素材按下拖出、右键菜单（含置灰）、重命名输入/回车与
//! 覆盖背景关闭。像素级外观由真实窗口截图负责。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::right_panel::RightSidebarAction;
use lumino_ui_egui::chrome::{
    self, ChromeModel, MaterialContextLabels, MaterialContextMenuItem, MaterialContextMenuModel,
    MaterialItemModel, MaterialMenuEntryModel, MaterialMenuKind, MaterialSectionModel,
    MaterialsPanelModel, RightSidebarModel, RightSidebarPanelKind, RightSidebarPanelModel,
    StatusbarModel,
};

fn statusbar() -> StatusbarModel {
    StatusbarModel {
        left: "就绪".to_owned(),
        left_accent: true,
        cpu_percent: None,
        memory_mb: 0.0,
        fps: None,
        right: String::new(),
    }
}

fn item(index: usize, name: &str, valid: bool) -> MaterialItemModel {
    MaterialItemModel {
        index,
        name: name.to_owned(),
        valid,
        renaming: None,
        tooltip: vec![format!("名称：{name}")],
    }
}

fn panel() -> MaterialsPanelModel {
    MaterialsPanelModel {
        title: "素材库".to_owned(),
        add_label: "+ 添加素材".to_owned(),
        add_menu: None,
        placeholder: None,
        rename_placeholder: "素材名称".to_owned(),
        sections: vec![
            MaterialSectionModel {
                label: "内置素材".to_owned(),
                entries: vec![item(0, "内置 A", true), item(1, "损坏素材", false)],
            },
            MaterialSectionModel {
                label: "用户素材".to_owned(),
                entries: vec![item(2, "我的素材", true)],
            },
        ],
        context_labels: MaterialContextLabels {
            rename: "重命名".to_owned(),
            delete: "删除".to_owned(),
            upload: "上传到云".to_owned(),
        },
        context_menu: None,
    }
}

fn chrome(model: MaterialsPanelModel) -> ChromeModel {
    ChromeModel {
        right_sidebar: Some(RightSidebarModel {
            panel_visible: true,
            panel_width: 200.0,
            active: Some(RightSidebarPanelKind::Materials),
            top_inset: 102.0,
            resizing: false,
            content: Some(RightSidebarPanelModel::Materials(model)),
        }),
        statusbar: statusbar(),
        ..ChromeModel::default()
    }
}

fn harness(model: ChromeModel) -> (Harness<'static>, Rc<RefCell<Vec<RightSidebarAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = chrome::right_panel::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

fn press(harness: &mut Harness<'static>, pos: egui::Pos2, button: egui::PointerButton) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
    press(harness, pos, egui::PointerButton::Primary);
}

fn rect(harness: &Harness<'static>, key: &str) -> egui::Pos2 {
    lumino_ui_egui::chrome::right_panel::rect(&harness.ctx, key)
        .unwrap_or_else(|| panic!("缺少命中区 {key}"))
        .center()
}

/// 添加按钮与下拉菜单项。
#[test]
fn add_button_and_menu_dispatch() {
    let (mut first, actions) = harness(chrome(panel()));
    let pos = rect(&first, "material:add");
    click(&mut first, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialAdd)
    );

    let mut with_menu = panel();
    with_menu.add_menu = Some(vec![
        MaterialMenuEntryModel {
            label: "从 web 下载".to_owned(),
            kind: MaterialMenuKind::DownloadFromWeb,
        },
        MaterialMenuEntryModel {
            label: "从本地选取".to_owned(),
            kind: MaterialMenuKind::ImportFromLocal,
        },
    ]);
    let (mut harness, actions) = harness(chrome(with_menu));
    let pos = rect(&harness, "material:menu:download");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialMenu(
            MaterialMenuKind::DownloadFromWeb
        ))
    );
    let pos = rect(&harness, "material:menu:local");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialMenu(
            MaterialMenuKind::ImportFromLocal
        ))
    );
}

/// 有效素材按下即开始拖出；无效素材无动作。
#[test]
fn item_press_starts_drag_only_when_valid() {
    let (mut harness, actions) = harness(chrome(panel()));
    let pos = rect(&harness, "material:item:0");
    click(&mut harness, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialDragStarted(0))
    );

    let before = actions.borrow().len();
    let pos = rect(&harness, "material:item:1");
    click(&mut harness, pos);
    assert_eq!(actions.borrow().len(), before, "无效素材不得开始拖出");
}

/// 右键素材项：回传索引与面板局部坐标。
#[test]
fn right_press_opens_context_menu_with_local_position() {
    let (mut harness, actions) = harness(chrome(panel()));
    let center = rect(&harness, "material:item:2");
    press(&mut harness, center, egui::PointerButton::Secondary);
    let action = actions.borrow().last().cloned();
    let Some(RightSidebarAction::MaterialContextMenuOpened { index, x, y }) = action else {
        panic!("应回传右键菜单打开动作，实际：{:?}", actions.borrow());
    };
    assert_eq!(index, 2);
    let content = lumino_ui_egui::chrome::right_panel::panel_rect(&harness.ctx).expect("面板");
    assert!((x + content.left() - center.x).abs() < 1.0, "x={x}");
    assert!((y + content.top() - center.y).abs() < 1.0, "y={y}");
}

/// 右键菜单项：可编辑时逐项回传；内置素材（置灰）无动作。
#[test]
fn context_menu_items_dispatch() {
    let mut model = panel();
    model.context_menu = Some(MaterialContextMenuModel {
        index: 2,
        can_edit: true,
        x: 40.0,
        y: 60.0,
    });
    let (mut first, actions) = harness(chrome(model));
    let pos = rect(&first, "material:ctx:rename");
    click(&mut first, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialContextMenuItemClicked {
            index: 2,
            item: MaterialContextMenuItem::Rename,
        })
    );
    let pos = rect(&first, "material:ctx:delete");
    click(&mut first, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialContextMenuItemClicked {
            index: 2,
            item: MaterialContextMenuItem::Delete,
        })
    );
    let pos = rect(&first, "material:ctx:upload");
    click(&mut first, pos);
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialContextMenuItemClicked {
            index: 2,
            item: MaterialContextMenuItem::UploadToCloud,
        })
    );

    // 内置素材：菜单全部置灰，点击不产生动作。
    let mut locked = panel();
    locked.context_menu = Some(MaterialContextMenuModel {
        index: 0,
        can_edit: false,
        x: 40.0,
        y: 60.0,
    });
    let (mut second, actions) = harness(chrome(locked));
    let before = actions.borrow().len();
    let pos = rect(&second, "material:ctx:delete");
    click(&mut second, pos);
    assert_eq!(actions.borrow().len(), before, "置灰菜单项不得产生动作");
}

/// 点击菜单外部空白：关闭菜单。
#[test]
fn context_menu_backdrop_closes() {
    let mut model = panel();
    model.context_menu = Some(MaterialContextMenuModel {
        index: 2,
        can_edit: true,
        x: 40.0,
        y: 60.0,
    });
    let (mut harness, actions) = harness(chrome(model));
    let panel = lumino_ui_egui::chrome::right_panel::panel_rect(&harness.ctx).expect("面板");
    // 标题区域，确定不在菜单面板内。
    click(
        &mut harness,
        egui::pos2(panel.left() + 20.0, panel.top() + 12.0),
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::MaterialContextMenuClosed)
    );
}

/// 重命名：输入变更 + 回车确认。
#[test]
fn rename_input_and_confirm() {
    let mut model = panel();
    model.sections[1].entries[0].renaming = Some("旧名称".to_owned());
    let (mut harness, actions) = harness(chrome(model));
    let pos = rect(&harness, "material:rename:2");
    click(&mut harness, pos);
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("A".to_owned()));
    harness.run();
    assert!(
        matches!(
            actions.borrow().last(),
            Some(RightSidebarAction::MaterialRenameInput(text)) if text.ends_with('A')
        ),
        "输入应回传，实际：{:?}",
        actions.borrow()
    );

    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, RightSidebarAction::MaterialRenameConfirmed)),
        "回车应确认重命名，实际：{:?}",
        actions.borrow()
    );
}
