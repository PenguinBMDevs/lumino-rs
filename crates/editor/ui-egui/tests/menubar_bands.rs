//! 系统原生标题栏模式的菜单栏 band（egui 接管 旧版 菜单）无头测试。
//!
//! 几何对齐 旧版 `menu::view_placeholder`：菜单从 x=0 开始、标签高 30、
//! 每个标签宽 = 文本宽 + 17（左右内边距 8+8 + 1px 间距），没有 logo /
//! 窗口控制 / 拖动区。交互与自绘标题栏共用同一套 `menu` 状态机。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, MenuBarModel, MenuEntry, MenuModel, StatusbarModel, TitlebarModel,
};

fn model(menu: Option<MenuBarModel>) -> ChromeModel {
    ChromeModel {
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec![
                "文件".to_owned(),
                "编辑".to_owned(),
                "视图".to_owned(),
                "帮助".to_owned(),
            ],
            menu,
            menu_only: true,
        }),
        statusbar: StatusbarModel {
            left: "就绪".to_owned(),
            left_accent: true,
            cpu_percent: None,
            memory_mb: 0.0,
            fps: None,
            right: String::new(),
            gpu: None,
        },
        ..ChromeModel::default()
    }
}

fn harness(model: ChromeModel) -> (Harness<'static>, Rc<RefCell<Option<usize>>>) {
    let action = Rc::new(RefCell::new(None));
    let action_w = action.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(index) = chrome::menu::take_action(ui.ctx()) {
                *action_w.borrow_mut() = Some(index);
            }
        });
    harness.run();
    (harness, action)
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

/// 菜单栏从 x=0 开始、高 30、标签首尾相接（无 logo / 无额外间隙）。
#[test]
fn native_menubar_starts_at_left_edge_without_logo() {
    let (harness, _) = harness(model(None));
    let rects = chrome::menu::label_rects(&harness.ctx);
    assert_eq!(rects.len(), 4, "四个主菜单");
    assert!(
        (rects[0].left() - 0.0).abs() < 0.5,
        "首个菜单必须贴左边缘，实际 {}",
        rects[0].left()
    );
    assert!((rects[0].top() - 0.0).abs() < 0.5);
    assert!(
        (rects[0].height() - 30.0).abs() < 0.5,
        "菜单栏高 30，实际 {}",
        rects[0].height()
    );
    for pair in rects.windows(2) {
        assert!(
            (pair[1].left() - pair[0].right()).abs() < 0.5,
            "标签之间不得有额外间隙：{:?}",
            pair
        );
    }
    // 4 个中文标签（每个文本宽 + 17）远小于「logo 48 + 4 标签」的自绘模式宽度。
    assert!(
        rects[3].right() < 250.0,
        "原生模式没有 logo，菜单总宽应只有标签，实际 {}",
        rects[3].right()
    );
}

/// 点击标签展开面板；点击菜单项回传动作序号并收起面板。
#[test]
fn native_menubar_click_opens_panel_and_dispatches() {
    let tree = MenuBarModel {
        menus: vec![MenuModel {
            label: "文件".to_owned(),
            entries: vec![MenuEntry::Action {
                label: "新建".to_owned(),
                enabled: true,
                action: 7,
            }],
        }],
    };
    let (mut harness, action) = harness(model(Some(tree)));
    let labels = chrome::menu::label_rects(&harness.ctx);
    click(&mut harness, labels[0].center());
    assert_eq!(
        chrome::menu::open_index(&harness.ctx),
        Some(0),
        "点击菜单标签应展开对应面板"
    );

    // 面板 = 本帧占用区里高度大于标题栏的那一块。
    let blocked = chrome::menu::take_blocked(&harness.ctx);
    let panel = blocked
        .into_iter()
        .find(|rect| rect.height() > 31.0)
        .expect("展开后应记录面板矩形");
    // 面板上下内边距 4、菜单项高 28。
    click(
        &mut harness,
        egui::pos2(panel.left() + 60.0, panel.top() + 4.0 + 14.0),
    );
    assert_eq!(*action.borrow(), Some(7), "菜单项点击应回传动作序号");
    assert_eq!(
        chrome::menu::open_index(&harness.ctx),
        None,
        "点击菜单项后面板应收起"
    );
}

/// 菜单面板必须独占命中：悬停在面板内边距（无菜单行处）不得点亮下层控件。
///
/// 回归：面板原先画在 root 图层里，内边距 / 分隔线没有控件，命中会落回
/// 下层的工具栏 band，表现为「菜单悬停穿透」。
#[test]
fn open_menu_blocks_hover_on_layers_below() {
    let tree = MenuBarModel {
        menus: vec![MenuModel {
            label: "文件".to_owned(),
            entries: vec![MenuEntry::Action {
                label: "新建".to_owned(),
                enabled: true,
                action: 3,
            }],
        }],
    };
    let under_hovered = std::rc::Rc::new(std::cell::Cell::new(true));
    let under = under_hovered.clone();
    let model = model(Some(tree));
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            // 模拟被菜单盖住的下层工具栏按钮。
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 30.0), egui::vec2(600.0, 72.0));
            let response = ui.interact(
                rect,
                egui::Id::unique("under-toolbar"),
                egui::Sense::click(),
            );
            under.set(response.hovered());
            chrome::show_bands(ui, &model, &mut icons);
        });
    harness.run();

    let labels = chrome::menu::label_rects(&harness.ctx);
    click(&mut harness, labels[0].center());
    let blocked = chrome::menu::take_blocked(&harness.ctx);
    let panel = blocked
        .into_iter()
        .find(|rect| rect.height() > 31.0)
        .expect("展开后应记录面板矩形");

    // 面板内边距 4px：这里没有菜单行，修复前命中会穿透到下层。
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(
            panel.left() + 1.0,
            panel.top() + 1.0,
        )));
    // 命中使用上一帧控件矩形：跑两帧让浮层进入命中集合。
    harness.run();
    harness.run();
    assert!(
        !under_hovered.get(),
        "菜单面板必须独占命中，下层控件不得 hover"
    );
}

/// 悬停切换到另一个标签后点击它：保持展开（不得被当作「再次点击关闭」）。
#[test]
fn hover_switch_then_click_keeps_menu_open() {
    let tree = MenuBarModel {
        menus: ["文件", "编辑", "视图", "帮助"]
            .into_iter()
            .map(|label| MenuModel {
                label: label.to_owned(),
                entries: vec![MenuEntry::Action {
                    label: "占位".to_owned(),
                    enabled: false,
                    action: 0,
                }],
            })
            .collect(),
    };
    let (mut harness, _) = harness(model(Some(tree)));
    let labels = chrome::menu::label_rects(&harness.ctx);
    click(&mut harness, labels[1].center());
    assert_eq!(chrome::menu::open_index(&harness.ctx), Some(1));

    // 指针移到「文件」：与 旧版 一致，展开的菜单跟随悬停切换。
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(labels[0].center()));
    harness.run();
    assert_eq!(
        chrome::menu::open_index(&harness.ctx),
        Some(0),
        "悬停到其他标签应切换展开的菜单"
    );

    // 再点击已因悬停而展开的标签：应保持展开，而不是立刻关闭。
    click(&mut harness, labels[0].center());
    assert_eq!(
        chrome::menu::open_index(&harness.ctx),
        Some(0),
        "悬停切换后点击同一标签不应关闭菜单"
    );
}

/// 原生模式没有窗口控制按钮：最右侧不应出现可点击的红绿灯区域。
#[test]
fn native_menubar_has_no_traffic_buttons() {
    let tree = MenuBarModel {
        menus: ["文件", "编辑", "视图", "帮助"]
            .into_iter()
            .map(|label| MenuModel {
                label: label.to_owned(),
                entries: Vec::new(),
            })
            .collect(),
    };
    let (harness, _) = harness(model(Some(tree)));
    // 菜单栏整宽 1600，但只在左侧标签上产生占用矩形。
    let blocked = chrome::menu::take_blocked(&harness.ctx);
    assert_eq!(blocked.len(), 4, "四个标签各一个占用矩形");
    assert!(
        blocked.iter().all(|rect| rect.right() < 250.0),
        "菜单栏占用区只应覆盖标签，实际 {blocked:?}"
    );
}
