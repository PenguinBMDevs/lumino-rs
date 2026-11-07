//! 外壳 band 的无头渲染测试：真实状态模型 → 可访问节点（菜单 / 状态栏文本）。
//!
//! 只验证「状态进来、文本出去」的接线与可见性规则；像素级对齐由
//! `bridge_parity`（四场景）与真实窗口截图负责。

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, MenuBarModel, MenuEntry, MenuModel, StatusbarModel, TitlebarModel,
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
        }),
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: None,
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
        }),
        sidebar: None,
        right_sidebar: None,
        piano_roll: None,
        toolbar: None,
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
