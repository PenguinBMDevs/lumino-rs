//! 文字参数面板（生产 `root/text_params_panel.rs` 的 egui 版）。
//!
//! 11 行固定高度：Font / Size / Weight / Italic / Unit / Tracking / Leading /
//! Align / Threshold / Grow / Mode；内容宽 220、面板宽 248、行距 6。
//! 字体下拉是自绘弹层（而不是 `ComboBox`），这样它的矩形也能进入占用列表，
//! 点击不会穿透到下层 iced。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use lumino_core::{TextAlign, TextLayoutMode, TextParams, TextUnit};

use super::super::TextParamsPanelModel;
use super::super::toolbar_popup::{self, ToolbarAction};
use super::panels::{self, TEXT};
use crate::theme;

mod font;

/// 内容宽度（生产 `CONTENT_WIDTH`）。
const CONTENT_W: f32 = 220.0;
/// 左侧标签宽度。
const LABEL_W: f32 = 62.0;
/// 滑杆宽度。
const SLIDER_W: f32 = 112.0;
/// 数值显示宽度。
const VALUE_W: f32 = 38.0;
/// 行高。
const ROW_H: f32 = 20.0;
/// 字体行高。
const FONT_ROW_H: f32 = 24.0;
/// 行间距。
const GAP: f32 = 6.0;
/// 面板内边距。
const PAD: f32 = 12.0;

/// 面板高度（11 行 + 间距 + 内边距）。
pub(super) fn height() -> f32 {
    PAD * 2.0 + FONT_ROW_H + ROW_H * 9.0 + 22.0 + GAP * 10.0
}

/// 绘制文字参数面板。
pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &TextParamsPanelModel,
    blocked: &mut Vec<Rect>,
) {
    let x = rect.left() + (rect.width() - CONTENT_W) / 2.0;
    let mut y = rect.top() + PAD;
    let params = &model.params;

    // Font 行：标签 + 自绘下拉（弹层在面板内容之后绘制，避免被后续行盖住）。
    label(ui, x, y, FONT_ROW_H, "Font");
    let field = Rect::from_min_size(
        Pos2::new(x + LABEL_W + 6.0, y),
        Vec2::new(150.0, FONT_ROW_H),
    );
    let font_open = font::field(ui, field, model);
    y += FONT_ROW_H + GAP;

    // Size：4..=96，步进 0.1。
    y = slider_row(
        ui,
        x,
        y,
        "Size",
        params.size,
        4.0..=96.0,
        0.1,
        |p, v| p.size = v,
        params,
    );

    // Weight：100..=900，步进 100（显示整数）。
    label(ui, x, y, ROW_H, "Weight");
    let weight = f32::from(params.weight);
    if let Some(value) = slider_value(
        ui,
        Rect::from_min_size(
            Pos2::new(x + LABEL_W + 4.0, y + (ROW_H - 18.0) / 2.0),
            Vec2::new(SLIDER_W, 18.0),
        ),
        weight,
        100.0..=900.0,
        100.0,
    ) {
        let mut next = params.clone();
        next.weight = value.round().clamp(100.0, 900.0) as u16;
        push_changed(ui, next);
    }
    value_text(ui, x, y, &format!("{}", params.weight));
    y += ROW_H + GAP;

    // Italic 开关。
    label(ui, x, y, ROW_H, "Italic");
    let button = Rect::from_min_size(
        Pos2::new(x + LABEL_W + 4.0, y),
        Vec2::new(panels::small_button_width(ui, "Italic"), ROW_H),
    );
    if panels::small_button(ui, button, "Italic", params.italic, "lumino.text.italic").clicked() {
        let mut next = params.clone();
        next.italic = !next.italic;
        push_changed(ui, next);
    }
    y += ROW_H + GAP;

    // Unit：Font / Rows。
    label(ui, x, y, ROW_H, "Unit");
    let mut bx = x + LABEL_W + 4.0;
    for (label_text, unit) in [("Font", TextUnit::Font), ("Rows", TextUnit::Rows)] {
        let button = Rect::from_min_size(
            Pos2::new(bx, y),
            Vec2::new(panels::small_button_width(ui, label_text), ROW_H),
        );
        let id = format!("lumino.text.unit.{label_text}");
        if panels::small_button(ui, button, label_text, params.unit == unit, id).clicked() {
            let mut next = params.clone();
            next.unit = unit;
            push_changed(ui, next);
        }
        bx += button.width() + 4.0;
    }
    y += ROW_H + GAP;

    // Tracking / Leading / Threshold / Grow。
    y = slider_row(
        ui,
        x,
        y,
        "Tracking",
        params.tracking,
        -20.0..=20.0,
        0.1,
        |p, v| p.tracking = v,
        params,
    );
    y = slider_row(
        ui,
        x,
        y,
        "Leading",
        params.leading,
        50.0..=200.0,
        0.1,
        |p, v| p.leading = v,
        params,
    );

    // Align：L / C / R。
    label(ui, x, y, ROW_H, "Align");
    let mut bx = x + LABEL_W + 4.0;
    for (label_text, align) in [
        ("L", TextAlign::Left),
        ("C", TextAlign::Center),
        ("R", TextAlign::Right),
    ] {
        let button = Rect::from_min_size(
            Pos2::new(bx, y),
            Vec2::new(panels::small_button_width(ui, label_text), ROW_H),
        );
        let id = format!("lumino.text.align.{label_text}");
        if panels::small_button(ui, button, label_text, params.align == align, id).clicked() {
            let mut next = params.clone();
            next.align = align;
            push_changed(ui, next);
        }
        bx += button.width() + 4.0;
    }
    y += ROW_H + GAP;

    y = slider_row(
        ui,
        x,
        y,
        "Threshold",
        params.threshold,
        0.0..=100.0,
        0.1,
        |p, v| p.threshold = v,
        params,
    );
    y = slider_row(
        ui,
        x,
        y,
        "Grow",
        params.grow,
        -10.0..=10.0,
        0.1,
        |p, v| p.grow = v,
        params,
    );

    // Mode：当前模式 + 切换按钮（LegacyBox ⇄ Parametric）。
    label(ui, x, y, 22.0, "Mode");
    let (current, target) = match model.mode {
        TextLayoutMode::LegacyBox => ("LegacyBox", "Parametric"),
        TextLayoutMode::Parametric => ("Parametric", "LegacyBox"),
    };
    let mode_text = ui.painter().text(
        Pos2::new(x + LABEL_W + 4.0, y + 11.0),
        Align2::LEFT_CENTER,
        current,
        FontId::proportional(12.0),
        TEXT,
    );
    let response = ui.interact(
        mode_text,
        egui::Id::unique("lumino.text.mode-hint"),
        Sense::hover(),
    );
    response.on_hover_text("LegacyBox：兼容旧框填充；Parametric：按文字参数排版。");
    let button = Rect::from_min_size(
        Pos2::new(x + LABEL_W + 4.0 + 70.0, y),
        Vec2::new(panels::small_button_width(ui, target) + 4.0, 22.0),
    );
    // 生产里这是普通 `button`（有底色），不是选中态小按钮。
    ui.painter().rect(
        button,
        CornerRadius::same(4),
        theme::TOOLBAR_BG,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    let response = ui.interact(
        button,
        egui::Id::unique("lumino.text.mode-switch"),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(button, CornerRadius::same(4), Color32::from_white_alpha(20));
    }
    ui.painter().text(
        button.center(),
        Align2::CENTER_CENTER,
        target,
        FontId::proportional(11.0),
        TEXT,
    );
    if response.clicked() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::TextLayoutModeToggle);
    }

    // 字体弹层最后画：盖住面板内容而不是被后续行覆盖。
    if font_open {
        font::popup(ui, field, model, blocked);
    }
}

/// 一行「标签 + 滑杆 + 数值」，返回下一行的 y。
#[allow(clippy::too_many_arguments)]
fn slider_row(
    ui: &mut egui::Ui,
    x: f32,
    y: f32,
    label_text: &'static str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    apply: impl Fn(&mut TextParams, f32),
    params: &TextParams,
) -> f32 {
    label(ui, x, y, ROW_H, label_text);
    if let Some(new_value) = slider_value(
        ui,
        Rect::from_min_size(
            Pos2::new(x + LABEL_W + 4.0, y + (ROW_H - 18.0) / 2.0),
            Vec2::new(SLIDER_W, 18.0),
        ),
        value,
        range,
        step,
    ) {
        let mut next = params.clone();
        apply(&mut next, new_value);
        push_changed(ui, next);
    }
    value_text(ui, x, y, &format!("{value:.1}"));
    y + ROW_H + GAP
}

/// 固定标签（宽 62、12px）。
fn label(ui: &egui::Ui, x: f32, y: f32, height: f32, text: &str) {
    ui.painter().text(
        Pos2::new(x, y + height / 2.0),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.0),
        TEXT,
    );
}

/// 右侧数值显示（11px、宽 38，右对齐行尾）。
fn value_text(ui: &egui::Ui, x: f32, y: f32, text: &str) {
    ui.painter().text(
        Pos2::new(
            x + LABEL_W + 4.0 + SLIDER_W + 4.0 + VALUE_W,
            y + ROW_H / 2.0,
        ),
        Align2::RIGHT_CENTER,
        text,
        FontId::proportional(11.0),
        TEXT,
    );
}

/// 滑杆（隐藏自带数值；返回变化后的值）。
fn slider_value(
    ui: &mut egui::Ui,
    rect: Rect,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
) -> Option<f32> {
    let mut value = value;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().slider_width = rect.width() - 18.0;
    let response = child.add(
        egui::Slider::new(&mut value, range)
            .show_value(false)
            .step_by(step),
    );
    response.changed().then_some(value)
}

/// 参数整体回传。
pub(super) fn push_changed(ui: &egui::Ui, params: TextParams) {
    toolbar_popup::push_action(ui.ctx(), ToolbarAction::TextParamsChanged(params));
}
