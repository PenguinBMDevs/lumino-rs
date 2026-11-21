//! 精度下拉按钮：宽度随吸附文本自动 resize（不靠省略号收敛）。
//!
//! 真机回归：`Bar` / `1/32 3` / 自定义 tick 曾因固定 57px 被省略号截断。这里用无头
//! harness 钉死两条契约：
//! 1. 触发区宽度 == `chrome::toolbar::precision::button_width`（绘制与折叠预算同源）；
//! 2. 按钮宽度 ≥ 真实字形宽度 + 图标 + 间距 + 内边距（文本永远放得下）。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{FontId, Rect, Vec2};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::toolbar::precision;
use lumino_ui_egui::chrome::{self, ChromeModel, ToolbarModel};
use lumino_ui_egui::icons::IconCache;
use lumino_ui_egui::{layout, theme};

/// 需要完整显示的档位：栏 / 四分 / 三连音 / 自定义 tick。
const SNAPS: &[&str] = &["Bar", "1/4", "1/32 3", "Custom 3840"];

/// 渲染一次工具栏，返回精度触发区矩形与吸附文本的真实字形宽度。
fn precision_band(snap: &'static str) -> (Rect, f32) {
    let model = ChromeModel {
        toolbar: Some(ToolbarModel {
            snap: snap.to_owned(),
            visible_tools: usize::MAX,
            ..ToolbarModel::default()
        }),
        ..ChromeModel::default()
    };
    let measured = Rc::new(RefCell::new(0.0_f32));
    let measured_w = measured.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1600.0, 120.0))
        .with_pixels_per_point(1.0)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            theme::apply(ctx);
            lumino_ui_egui::fonts::install(ctx);
        })
        .build_ui(move |ui| {
            *measured_w.borrow_mut() = ui
                .painter()
                .layout_no_wrap(
                    snap.to_owned(),
                    FontId::proportional(theme::font::BODY),
                    theme::text(),
                )
                .size()
                .x;
            chrome::show_bands(ui, &model, &mut icons);
        });
    harness.run();
    let rect = chrome::toolbar_popup::snap_trigger(&harness.ctx).expect("精度框应发布触发矩形");
    (rect, *measured.borrow())
}

/// 用真实字体度量一组文本（`(文本, 字号)`），返回逻辑宽度。
fn measure(requests: &[(&str, f32)]) -> Vec<f32> {
    let requests = requests.to_vec();
    let out = Rc::new(RefCell::new(Vec::new()));
    let out_w = out.clone();
    let mut harness = Harness::builder()
        .with_size(Vec2::new(400.0, 100.0))
        .with_pixels_per_point(1.0)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            theme::apply(ctx);
            lumino_ui_egui::fonts::install(ctx);
        })
        .build_ui(move |ui| {
            let widths: Vec<f32> = requests
                .iter()
                .map(|(text, size)| {
                    ui.painter()
                        .layout_no_wrap(
                            (*text).to_owned(),
                            FontId::proportional(*size),
                            theme::text(),
                        )
                        .size()
                        .x
                })
                .collect();
            *out_w.borrow_mut() = widths;
        });
    harness.run();
    out.borrow().clone()
}

/// 每个档位都必须完整显示：按钮宽度 == 预算宽度，且放得下真实文本。
#[test]
fn precision_box_fits_every_snap_text() {
    for snap in SNAPS {
        let (rect, measured) = precision_band(snap);
        let expected = precision::button_width(snap);
        assert!(
            (rect.width() - expected).abs() < 0.01,
            "{snap}: 触发区宽度 {} 与折叠预算 {expected} 不一致",
            rect.width()
        );
        let need = measured
            + layout::PRECISION_ICON
            + layout::PRECISION_ICON_GAP
            + layout::PRECISION_PAD_X * 2.0;
        assert!(
            rect.width() >= need,
            "{snap}: 按钮宽 {} 放不下文本 {measured:.2} + 图标/内边距 {need:.2}（会被省略号截断）",
            rect.width()
        );
    }
}

/// 宽度随文本单调增长；预设档位只允许合理增大，不产生抖动。
#[test]
fn precision_box_grows_with_text() {
    let (quarter, _) = precision_band("1/4");
    let (triplet, _) = precision_band("1/32 3");
    let (custom, _) = precision_band("Custom 3840");

    assert!(
        quarter.width() >= layout::PRECISION_W,
        "1/4 不应窄于历史下限 {}（实际 {}）",
        layout::PRECISION_W,
        quarter.width()
    );
    assert!(
        quarter.width() < 62.0,
        "1/4 只允许合理增大（实际 {}）",
        quarter.width()
    );
    assert!(
        quarter.width() < triplet.width(),
        "三连音 {} 应宽于 1/4 {}",
        triplet.width(),
        quarter.width()
    );
    assert!(
        triplet.width() < custom.width(),
        "自定义 tick {} 应宽于三连音 {}",
        custom.width(),
        triplet.width()
    );
}

/// 宽度上界必须覆盖所有预设文本（估算击穿 = 按钮截断）。
#[test]
fn width_upper_bound_covers_every_preset() {
    let mut texts: Vec<String> = lumino_core::SnapSpec::presets()
        .into_iter()
        .map(|spec| spec.text())
        .collect();
    texts.push("Custom 3840".to_owned());
    texts.push("Custom 4294967295".to_owned());

    let requests: Vec<(&str, f32)> = texts
        .iter()
        .map(|text| (text.as_str(), theme::font::BODY))
        .collect();
    for (text, measured) in texts.iter().zip(measure(&requests)) {
        let upper = precision::text_width_upper(text, theme::font::BODY);
        assert!(
            upper >= measured,
            "{text:?}: 上界 {upper:.2} < 实测 {measured:.2}，按钮会截断"
        );
    }
}

/// 标签预留（中 / 英）同样必须覆盖真实字形宽度。
#[test]
fn label_reserve_covers_both_languages() {
    let labels = ["精度:", "Precision:"];
    let requests: Vec<(&str, f32)> = labels
        .iter()
        .map(|label| (*label, theme::font::LABEL))
        .collect();
    for (label, measured) in labels.iter().zip(measure(&requests)) {
        let reserve = precision::label_width(label);
        assert!(
            reserve >= measured,
            "{label:?}: 预留 {reserve:.2} < 实测 {measured:.2}"
        );
    }
}
