//! 指针归属不变量：egui 面板必须**自己**占住命中，不依赖单一数据源。
//!
//! 生产口径（`Host::egui_owns_pointer`）是**两条独立判定取并集**：
//!
//! 1. `blocked` 占用表：手工登记的矩形（band / 根层手绘浮层 / 浮层首帧兜底）；
//! 2. `layer_id_at` 非 `Background` 图层：`Area` / `Window` / ComboBox 弹层。
//!
//! 两条都不能省：`Area` **第一次出现的帧**是 sizing pass（`interactable == false`），
//! `layer_id_at` 命中不到；只依赖图层会留下「打开下拉首帧点击穿透」的窗口。
//! 反之只靠占用表则会漏掉未登记的新弹层。
//!
//! 这里把「覆盖」拆成独立断言，任何一条失效都会先在这个测试里失败：
//!
//! 1. `Area` 弹层：`layer_id_at` 必须返回非 `Background` 图层（自占）；
//! 2. `Area` 弹层：`blocked` 必须包含面板矩形（首帧兜底）；
//! 3. band 内的触发按钮：必须被 band 级占用矩形覆盖。
use egui::{Pos2, Rect};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, SnapMenuItem, SnapMenuModel, StatusbarModel, ToolbarModel,
};
use lumino_ui_egui::icons::IconCache;

fn base_model() -> ChromeModel {
    ChromeModel {
        titlebar: None,
        toolbar: Some(ToolbarModel {
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
            gpu: None,
            memory_tip: Default::default(),
        },
        ..ChromeModel::default()
    }
}

fn harness(model: ChromeModel) -> Harness<'static> {
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
        });
    // 第一帧是 `Area` 的 sizing pass（`interactable` 临时为 false），
    // 跑够几帧让它进入命中栈——生产侧每帧都会重画，等价。
    for _ in 0..3 {
        harness.run();
    }
    harness
}

/// 生产口径：`blocked ∪ 非 Background 图层`。
fn owned(harness: &Harness<'static>, blocked: &[Rect], pos: Pos2) -> bool {
    blocked.iter().any(|rect| rect.contains(pos))
        || harness
            .ctx
            .layer_id_at(pos)
            .is_some_and(|layer| layer.order != egui::Order::Background)
}

fn snap_items() -> Vec<SnapMenuItem> {
    lumino_core::SnapSpec::presets()
        .into_iter()
        .take(3)
        .map(|spec| SnapMenuItem {
            spec,
            label: spec.text(),
            selected: false,
        })
        .collect()
}

/// `Area` 弹层必须自占：`layer_id_at` 覆盖面板中心，且占用表同步登记。
///
/// 前者是稳定态主判定，后者兜住 `Area` 首帧 sizing pass——缺一就会穿透。
#[test]
fn floating_panel_owns_its_layer_without_manual_rects() {
    let harness = harness(ChromeModel {
        toolbar: Some(ToolbarModel {
            visible_tools: usize::MAX,
            snap_menu: Some(SnapMenuModel {
                items: snap_items(),
                custom_label: "Custom…".to_owned(),
            }),
            ..ToolbarModel::default()
        }),
        ..base_model()
    });

    let trigger = chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("精度框应发布触发矩形");
    let panel = chrome::toolbar_popup::snap_panel_rect(trigger, snap_items().len(), true);

    // 面板中心与靠近边缘的内边距处都必须被图层覆盖（内边距没有控件）。
    let blocked = chrome::menu::take_blocked(&harness.ctx);
    for pos in [
        panel.center(),
        Pos2::new(panel.left() + 2.0, panel.top() + 2.0),
    ] {
        assert!(
            harness
                .ctx
                .layer_id_at(pos)
                .is_some_and(|layer| layer.order != egui::Order::Background),
            "弹层必须自占命中（layer_id_at），{pos:?} 落回了 Background"
        );
        assert!(
            blocked.iter().any(|rect| rect.contains(pos)),
            "弹层必须同步登记占用表（首帧兜底），{pos:?} 不在 {blocked:?}"
        );
    }
}

/// band 内的触发按钮必须被 band 级占用矩形覆盖。
///
/// 删除 `toolbar_popup` 的整块登记后，触发区不能失去保护。
#[test]
fn toolbar_trigger_stays_owned_by_band_blocking() {
    let harness = harness(base_model());
    let trigger = chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("精度框应发布触发矩形");
    let onion = chrome::toolbar_popup::onion_triggers(&harness.ctx);

    let blocked = chrome::menu::take_blocked(&harness.ctx);
    assert!(
        owned(&harness, &blocked, trigger.center()),
        "精度框中心必须被占用（band 或图层），否则点击会落到画布"
    );
    if let Some((button, caret)) = onion {
        assert!(
            owned(&harness, &blocked, button.center()),
            "洋葱皮主按钮必须被占用"
        );
        assert!(
            owned(&harness, &blocked, caret.center()),
            "洋葱皮下拉三角必须被占用"
        );
    }
}

/// 弹层打开时，面板与触发区都必须在占用集合内（合成不变量）。
#[test]
fn open_snap_menu_owns_panel_and_trigger() {
    let items = snap_items();
    let count = items.len();
    let harness = harness(ChromeModel {
        toolbar: Some(ToolbarModel {
            visible_tools: usize::MAX,
            snap_menu: Some(SnapMenuModel {
                items,
                custom_label: "Custom…".to_owned(),
            }),
            ..ToolbarModel::default()
        }),
        ..base_model()
    });

    let trigger = chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("精度框");
    let panel = chrome::toolbar_popup::snap_panel_rect(trigger, count, true);
    let blocked = chrome::menu::take_blocked(&harness.ctx);

    for (name, pos) in [("触发区", trigger.center()), ("面板中心", panel.center())] {
        assert!(
            owned(&harness, &blocked, pos),
            "{name} 必须被 egui 占用，否则同一次点击会同时落到画布"
        );
    }
}
