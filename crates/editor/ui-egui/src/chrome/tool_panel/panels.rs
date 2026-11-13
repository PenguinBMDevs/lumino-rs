//! 悬浮条锚定面板：画刷 / 形状 / 分音符填充 / 粗细（文字参数见 `text_panel`）。
//!
//! 全部手工排版（固定行高），几何来自生产：
//! - 面板宽 248、圆角 8、1px `background.strong` 边框、底色 = 工具栏底色 ×0.9；
//! - 内边距 12（形状 8、填充 10）、行间距 8（文字 6）。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::toolbar_popup::{self, ToolbarAction};
use super::super::{
    BrushPanelModel, ShapeKind, ShapePanelModel, ThicknessPanelModel, ToolPanelSettingsPanel,
};
use super::text_panel;

mod fill;
use crate::icons::IconCache;
use crate::theme;

/// 文字主色（深色面板上的可读色；对应生产 `contrast_text_color`）。
pub(super) const TEXT: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF2);
/// 说明文字（略暗）。
pub(super) const TEXT_DIM: Color32 = Color32::from_rgb(0xB8, 0xBC, 0xC8);
/// 校验错误红。
const ERROR: Color32 = Color32::from_rgb(0xFF, 0x73, 0x73);
/// 小按钮选中底色（生产 `rgba(0.22, 0.42, 0.62, 0.9)`）。
const SELECTED: Color32 = Color32::from_rgb(0x38, 0x6B, 0x9E);

/// 面板内边距 / 行高。
const PAD: f32 = 12.0;
const ROW: f32 = 18.0;
const BTN_H: f32 = 26.0;
const INPUT_H: f32 = 26.0;
const HINT_H: f32 = 14.0;
const SLIDER_H: f32 = 20.0;

/// 形状面板按钮尺寸（生产 `BUTTON_SIZE` / `ICON_SIZE`）。
const SHAPE_BTN: f32 = 40.0;
const SHAPE_ICON: f32 = 36.0;

/// 面板底色：工具栏底色（`background.weakest`）压暗 10%。
pub(super) fn panel_bg() -> Color32 {
    let base = theme::panel_bg();
    let dim = |c: u8| (c as f32 * 0.9) as u8;
    Color32::from_rgb(dim(base.r()), dim(base.g()), dim(base.b()))
}

/// 面板高度（供锚定定位；由固定行高算出，与绘制一一对应）。
pub(super) fn panel_height(panel: &ToolPanelSettingsPanel) -> f32 {
    match panel {
        ToolPanelSettingsPanel::Brush(model) => {
            PAD * 2.0
                + ROW
                + if model.show_behavior {
                    8.0 + BTN_H
                } else {
                    0.0
                }
        }
        ToolPanelSettingsPanel::Shape(_) => 8.0 * 2.0 + SHAPE_BTN,
        ToolPanelSettingsPanel::FillDivision(model) => {
            10.0 * 2.0
                + INPUT_H
                + 4.0
                + HINT_H
                + if model.error.is_some() { 12.0 } else { 0.0 }
                + 8.0
                + BTN_H
        }
        ToolPanelSettingsPanel::TextParams(_) => text_panel::height(),
        ToolPanelSettingsPanel::Thickness(_) => PAD * 2.0 + ROW + 8.0 + SLIDER_H + 8.0 + HINT_H,
    }
}

/// 绘制一个面板（背景 + 内容），占用矩形交给调用方发布。
pub(super) fn show(
    ui: &mut egui::Ui,
    panel: &ToolPanelSettingsPanel,
    rect: Rect,
    blocked: &mut Vec<Rect>,
    icons: &mut IconCache,
) {
    blocked.push(rect);
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        panel_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    match panel {
        ToolPanelSettingsPanel::Brush(model) => brush(ui, rect, model, icons),
        ToolPanelSettingsPanel::Shape(model) => shape(ui, rect, model, icons),
        ToolPanelSettingsPanel::FillDivision(model) => fill::show(ui, rect, model),
        ToolPanelSettingsPanel::TextParams(model) => text_panel::show(ui, rect, model, blocked),
        ToolPanelSettingsPanel::Thickness(model) => thickness(ui, rect, model),
    }
}

/// 画刷面板：粗细只读行 + 「绘制行为」入口（粗细 > 1 时）。
fn brush(ui: &mut egui::Ui, rect: Rect, model: &BrushPanelModel, icons: &mut IconCache) {
    let x = rect.left() + PAD;
    let mut y = rect.top() + PAD;
    let title_w = text_width(ui, &model.title, 14.0);
    ui.painter().text(
        Pos2::new(x, y + ROW / 2.0),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(14.0),
        TEXT,
    );
    ui.painter().text(
        Pos2::new(x + title_w + 8.0, y + ROW / 2.0),
        Align2::LEFT_CENTER,
        &model.value,
        FontId::proportional(14.0),
        TEXT,
    );
    y += ROW;

    if model.show_behavior {
        y += 8.0;
        let label_w = text_width(ui, &model.behavior, 14.0);
        let button = Rect::from_min_size(
            Pos2::new(x, y),
            Vec2::new(8.0 + 18.0 + 4.0 + label_w + 4.0, BTN_H),
        );
        let response = ui.interact(
            button,
            egui::Id::unique("lumino.brush-behavior"),
            Sense::click(),
        );
        ui.painter().rect_filled(
            button,
            CornerRadius::same(4),
            if response.hovered() {
                Color32::from_white_alpha(26)
            } else {
                Color32::TRANSPARENT
            },
        );
        let gear = Rect::from_center_size(
            Pos2::new(button.left() + 4.0 + 9.0, button.center().y),
            Vec2::splat(18.0),
        );
        icons.paint(ui.painter(), crate::icons::Icon::Gear, gear);
        ui.painter().text(
            Pos2::new(gear.right() + 4.0, button.center().y),
            Align2::LEFT_CENTER,
            &model.behavior,
            FontId::proportional(14.0),
            TEXT,
        );
        if response.clicked() {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::BrushSettingsOpen);
        }
    }
}

/// 形状面板：矩形 / 圆形 / 三角形 + 形状抽屉。
fn shape(ui: &mut egui::Ui, rect: Rect, model: &ShapePanelModel, icons: &mut IconCache) {
    let entries = [
        (
            ShapeKind::Rectangle,
            crate::icons::Icon::ShapeRectangle,
            &model.rectangle,
        ),
        (
            ShapeKind::Circle,
            crate::icons::Icon::ShapeCircle,
            &model.circle,
        ),
        (
            ShapeKind::Triangle,
            crate::icons::Icon::ShapeTriangle,
            &model.triangle,
        ),
    ];
    let mut x = rect.left() + 8.0;
    let y = rect.top() + 8.0;
    for (kind, icon, tooltip) in entries {
        let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(SHAPE_BTN));
        let selected = kind == model.selected;
        let response = shape_cell(ui, icons, cell, icon, selected, kind as u8, SHAPE_ICON);
        let response = response.on_hover_text(tooltip);
        if response.clicked() {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::ShapeSelected(kind));
        }
        x += SHAPE_BTN + 4.0;
    }
    let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(SHAPE_BTN));
    let response = shape_cell(
        ui,
        icons,
        cell,
        crate::icons::Icon::Gear,
        false,
        9,
        SHAPE_ICON,
    );
    let response = response.on_hover_text(&model.drawer);
    if response.clicked() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::OpenShapeDrawer);
    }
}

/// 形状按钮（图标由调用方按 36px 缓存；此处用占位方块保持命中区）。
fn shape_cell(
    ui: &mut egui::Ui,
    icons: &mut IconCache,
    rect: Rect,
    icon: crate::icons::Icon,
    selected: bool,
    id: u8,
    icon_size: f32,
) -> egui::Response {
    let response = ui.interact(
        rect,
        egui::Id::unique(("lumino.shape-cell", id)),
        Sense::click(),
    );
    let background = if selected {
        Some(Color32::from_white_alpha(41))
    } else if response.is_pointer_button_down_on() {
        Some(Color32::from_white_alpha(56))
    } else if response.hovered() {
        Some(Color32::from_white_alpha(31))
    } else {
        None
    };
    if let Some(background) = background {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), background);
    }
    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
    icons.paint(ui.painter(), icon, icon_rect);
    response
}

/// 粗细面板：信息行 + 滑杆 + 说明。
fn thickness(ui: &mut egui::Ui, rect: Rect, model: &ThicknessPanelModel) {
    let x = rect.left() + PAD;
    let mut y = rect.top() + PAD;
    let title_w = text_width(ui, &model.title, 14.0);
    ui.painter().text(
        Pos2::new(x, y + ROW / 2.0),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(14.0),
        TEXT,
    );
    ui.painter().text(
        Pos2::new(x + title_w + 8.0, y + ROW / 2.0),
        Align2::LEFT_CENTER,
        &model.value_text,
        FontId::proportional(14.0),
        TEXT,
    );
    y += ROW + 8.0;

    let mut value = f32::from(model.value);
    let slider = Rect::from_min_size(Pos2::new(x, y), Vec2::new(200.0, SLIDER_H));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(slider)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    let response = child.add(
        egui::Slider::new(&mut value, f32::from(model.min)..=f32::from(model.max))
            .show_value(false)
            .step_by(1.0),
    );
    if response.changed() {
        let value = value
            .round()
            .clamp(f32::from(model.min), f32::from(model.max)) as u8;
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ThicknessChanged(value));
    }
    y += SLIDER_H + 8.0;

    ui.painter().text(
        Pos2::new(x, y + HINT_H / 2.0),
        Align2::LEFT_CENTER,
        &model.hint,
        FontId::proportional(11.0),
        TEXT_DIM,
    );
}

/// 文本宽度（与生产度量同口径：egui 比例字体）。
pub(super) fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), TEXT)
        .size()
        .x
}

/// 小按钮选中底色（供文字参数面板复用）。
pub(super) fn selection_fill() -> Color32 {
    SELECTED
}

/// 小按钮（文字参数面板用）：选中填色，其余透明。
pub(super) fn small_button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    selected: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> egui::Response {
    let response = ui.interact(rect, egui::Id::unique(id), Sense::click());
    let background = if selected {
        SELECTED
    } else if response.hovered() {
        Color32::from_white_alpha(20)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), background);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        TEXT,
    );
    response
}

/// 小按钮宽度（文字 + 左右 6）。
pub(super) fn small_button_width(ui: &egui::Ui, label: &str) -> f32 {
    text_width(ui, label, 11.0) + 12.0
}
