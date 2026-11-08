//! 外壳 band 的无头渲染测试：真实状态模型 → 可访问节点（菜单 / 状态栏文本）。
//!
//! 只验证「状态进来、文本出去」的接线与可见性规则；像素级对齐由
//! `bridge_parity`（四场景）与真实窗口截图负责。

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_core::SnapSpec;
use lumino_ui_egui::chrome::{
    self, ChromeModel, MenuBarModel, MenuEntry, MenuModel, OnionMenuModel, SnapMenuItem,
    SnapMenuModel, StatusbarModel, TitlebarModel, ToolbarAction, ToolbarModel,
};
use lumino_ui_egui::icons::IconCache;

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

fn harness(model: ChromeModel) -> Harness<'static> {
    let mut icons = IconCache::default();
    Harness::builder()
        .with_size(egui::vec2(960.0, 620.0))
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons))
}

#[test]
fn bands_expose_menu_labels_and_status_metrics() {
    let mut harness = harness(ChromeModel {
        dialog: None,
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec![
                "文件".to_owned(),
                "编辑".to_owned(),
                "视图".to_owned(),
                "帮助".to_owned(),
            ],
            menu: None,
            menu_only: false,
        }),
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: None,
        tool_panel: None,
        statusbar: statusbar(Some(58.5)),
    });
    harness.run();

    for label in ["文件", "编辑", "视图", "帮助"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "标题栏菜单 {label} 未出现在可访问树中"
        );
    }
    for text in ["就绪", "CPU", "12.5%", "MEM", "256.0 MB", "FPS", "58.5"] {
        assert!(
            harness.query_by_label(text).is_some(),
            "状态栏文本 {text} 未出现在可访问树中"
        );
    }
}

#[test]
fn native_titlebar_skips_top_band_but_keeps_statusbar() {
    let mut harness = harness(ChromeModel {
        dialog: None,
        titlebar: None,
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: None,
        tool_panel: None,
        statusbar: statusbar(Some(60.0)),
    });
    harness.run();

    assert!(
        harness.query_by_label("文件").is_none(),
        "系统原生标题栏模式下不该绘制 egui 菜单"
    );
    assert!(
        harness.query_by_label("就绪").is_some(),
        "标题栏关闭时状态栏仍必须渲染"
    );
}

#[test]
fn missing_fps_hides_metric_group() {
    let mut harness = harness(ChromeModel {
        dialog: None,
        titlebar: None,
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: None,
        tool_panel: None,
        statusbar: statusbar(None),
    });
    harness.run();

    assert!(harness.query_by_label("就绪").is_some());
    assert!(
        harness.query_by_label("CPU").is_none(),
        "FPS 未采样时生产逻辑隐藏整组指标"
    );
    assert!(harness.query_by_label("FPS").is_none());
}

/// 菜单交互：点标签展开 → 点禁用项无动作 → 悬停子菜单 → 点动作项产生序号。
#[test]
fn menu_opens_and_dispatches_actions() {
    let tree = MenuBarModel {
        menus: vec![MenuModel {
            label: "文件".to_owned(),
            entries: vec![
                MenuEntry::Action {
                    label: "新建".to_owned(),
                    enabled: true,
                    action: 0,
                },
                MenuEntry::Action {
                    label: "打开".to_owned(),
                    enabled: false,
                    action: 1,
                },
                MenuEntry::Separator,
                MenuEntry::Submenu {
                    label: "导出".to_owned(),
                    entries: vec![MenuEntry::Action {
                        label: "导出工程".to_owned(),
                        enabled: true,
                        action: 2,
                    }],
                },
            ],
        }],
    };
    let model = ChromeModel {
        dialog: None,
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec!["文件".to_owned()],
            menu: Some(tree),
            menu_only: false,
        }),
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: None,
        tool_panel: None,
        statusbar: statusbar(None),
    };

    // 闭包里读 egui 内存：展开索引与点击动作。
    let observed = std::rc::Rc::new(std::cell::RefCell::new((None, None)));
    let observed_w = observed.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(960.0, 620.0))
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            let open = chrome::menu::open_index(ui.ctx());
            let action = chrome::menu::take_action(ui.ctx()).or(observed_w.borrow().1);
            *observed_w.borrow_mut() = (open, action);
        });
    harness.run();
    assert_eq!(observed.borrow().0, None, "初始不应有展开菜单");

    // 第 1 个标签中心：logo 区 48 宽（15 + 18 + 15），标签宽约 45。
    click(&mut harness, egui::pos2(70.0, 15.0));
    assert_eq!(observed.borrow().0, Some(0), "点击文件应展开菜单");

    let image = harness.render().expect("菜单测试需要可用的渲染器");
    let panel = image.get_pixel(60, 40).0;
    assert_eq!(
        &panel[..3],
        &lumino_ui_egui::theme::CARD_BG.to_array()[..3],
        "下拉面板应画在标题栏下方（面板底色 background.base）"
    );

    // 第 2 行（禁用）点击：不产生动作，菜单保持展开。
    click(&mut harness, egui::pos2(75.0, 78.0));
    assert_eq!(observed.borrow().1, None, "禁用项不得产生动作");
    assert_eq!(observed.borrow().0, Some(0), "点禁用项不应关闭菜单");

    // 悬停子菜单行（第 4 行）→ 子面板展开；点其唯一项。
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(75.0, 115.0)));
    harness.run();
    click(&mut harness, egui::pos2(300.0, 119.0));
    assert_eq!(observed.borrow().1, Some(2), "子菜单项应回传动作序号 2");
    assert_eq!(observed.borrow().0, None, "点击动作后菜单关闭");
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

/// 工具栏弹层：精度框点击 → 开合动作；预设行 → 具体 `SnapSpec`；
/// 洋葱皮下三角 → 开合动作；范围行 → `OnionMode(index)`。
#[test]
fn toolbar_popups_dispatch_snap_and_onion_actions() {
    let model = std::rc::Rc::new(std::cell::RefCell::new(ChromeModel {
        dialog: None,
        titlebar: None,
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: Some(ToolbarModel::default()),
        tool_panel: None,
        statusbar: statusbar(None),
    }));
    let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut icons = IconCache::default();
    let model_w = model.clone();
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        // 工具栏固定分组宽：960px 会把精度框排到视口外，取真实窗口量级。
        .with_size(egui::vec2(1600.0, 900.0))
        // 工具栏图标按钮没有可访问名称（生产版也没有）；本测试只验证点击路由。
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model_w.borrow(), &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    // 精度框：点击回传开合动作。
    let snap_trigger =
        chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("工具栏应记录精度框矩形");
    click(&mut harness, snap_trigger.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ToggleSnapPicker),
        "点精度框应请求开合下拉"
    );

    // 模拟生产打开下拉（下一帧由生产状态驱动），点第一行预设。
    let quarter = SnapSpec::Note {
        notes: 4,
        dots: 0,
        div: 1,
    };
    model
        .borrow_mut()
        .toolbar
        .as_mut()
        .expect("工具栏")
        .snap_menu = Some(SnapMenuModel {
        items: vec![SnapMenuItem {
            spec: quarter,
            label: "1/4".to_owned(),
            selected: true,
        }],
        custom_label: "Custom…".to_owned(),
    });
    harness.run();
    let panel = chrome::toolbar_popup::snap_panel_rect(snap_trigger, 1, true);
    let row = egui::pos2(panel.center().x, panel.top() + 6.0 + 12.0);
    click(&mut harness, row);
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::SnapSpec(quarter)),
        "点预设行应回传具体吸附规格"
    );

    // 自定义行 → 打开自定义精度对话框。
    let custom_row = egui::pos2(panel.center().x, panel.top() + 6.0 + 24.0 + 2.0 + 12.0);
    click(&mut harness, custom_row);
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::SnapCustom),
        "点 Custom… 应请求打开自定义精度对话框"
    );

    // 洋葱皮下三角 → 开合动作；范围行 → 模式下标。
    let (_, caret) =
        chrome::toolbar_popup::onion_triggers(&harness.ctx).expect("工具栏应记录洋葱皮触发器");
    click(&mut harness, caret.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ToggleOnionMenu),
        "点下三角应请求开合洋葱皮范围菜单"
    );

    model
        .borrow_mut()
        .toolbar
        .as_mut()
        .expect("工具栏")
        .onion_menu = Some(OnionMenuModel {
        items: (0..4)
            .map(|index| lumino_ui_egui::chrome::OnionMenuItem {
                label: format!("模式 {index}"),
                selected: index == 0,
            })
            .collect(),
    });
    harness.run();
    let onion_panel = chrome::toolbar_popup::onion_panel_rect(caret, 4);
    let second = egui::pos2(
        onion_panel.center().x,
        onion_panel.top() + 6.0 + 22.0 + 2.0 + 11.0,
    );
    click(&mut harness, second);
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::OnionMode(1)),
        "点第二个范围应回传下标 1"
    );
}
