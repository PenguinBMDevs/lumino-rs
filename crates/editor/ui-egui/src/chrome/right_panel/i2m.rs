//! 右侧栏「图片转 MIDI」面板渲染。
//!
//! 几何对齐生产 `right_sidebar/view/i2m_panel.rs`：
//! - 内容内边距 8、列间距 8；标题 14px、配置小节 12px、输入框字号 12；
//! - 输入框 42×22（`padding [3,6]`、`background.weak` 底、1px `background.strong`
//!   边框、圆角 4）；调色板下拉 118 宽；
//! - 「选择图片文件」与「转换为 MIDI」为标准按钮（`padding 6`、圆角 4、
//!   `background.weak` 底、悬停 `background.base` / 主色）。

use egui::{
    Align, Align2, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText, Stroke, Vec2,
};

use super::super::model::{I2mControlModel, I2mField, I2mPanelModel, I2mRowModel};
use super::widgets::{self, ButtonStyle};
use super::{RightSidebarAction, push_action, record_rect};
use crate::theme;

const PAD: f32 = 8.0;
const GAP: f32 = 8.0;
const ROW_GAP: f32 = 4.0;
const HEADER_H: f32 = 20.0;
const SECTION_H: f32 = 18.0;
const ROW_H: f32 = 22.0;
const BUTTON_H: f32 = 28.0;
const INPUT_W: f32 = 42.0;
const COMBO_W: f32 = 118.0;

/// 内容高度（供 ScrollArea 预留；配置行数固定，按模型计算）。
fn content_height(panel: &I2mPanelModel) -> f32 {
    let rows = panel.rows.len() as f32;
    let convert = panel
        .convert
        .as_ref()
        .map(|_| GAP + BUTTON_H)
        .unwrap_or(0.0);
    PAD * 2.0
        + HEADER_H
        + GAP
        + BUTTON_H
        + GAP
        + SECTION_H
        + ROW_GAP
        + rows * ROW_H
        + rows.max(1.0) * ROW_GAP
        + convert
}

/// 绘制面板（`rect` = 手柄右侧的内容区）。
pub(super) fn show(ui: &mut egui::Ui, rect: Rect, model: &I2mPanelModel) {
    let total = content_height(model);
    widgets::scroll_body(
        ui,
        rect,
        "lumino.right-sidebar.i2m",
        total,
        |ui, content| {
            if content.width() <= PAD * 2.0 {
                return;
            }
            draw_panel(ui, content, model);
        },
    );
}

fn draw_panel(ui: &mut egui::Ui, rect: Rect, model: &I2mPanelModel) {
    let left = rect.left() + PAD;
    let right = rect.right() - PAD;
    let width = (right - left).max(0.0);
    let mut y = rect.top() + PAD;

    // 标题。
    ui.painter().text(
        Pos2::new(left, y + HEADER_H / 2.0),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(theme::font::LABEL),
        theme::text_on_weak(),
    );
    y += HEADER_H + GAP;

    // 选择图片按钮 + 已选提示。
    let select_w = widgets::text_width(ui, &model.select_image, theme::font::BODY) + 14.0;
    let select = Rect::from_min_size(Pos2::new(left, y), Vec2::new(select_w, BUTTON_H));
    if widgets::button(
        ui,
        select,
        &model.select_image,
        ButtonStyle {
            fill: theme::toolbar_bg(),
            hover: theme::base_bg(),
            text: theme::text(),
            enabled: true,
        },
    )
    .clicked()
    {
        push_action(ui.ctx(), RightSidebarAction::SelectImageFile);
    }
    if let Some(hint) = &model.selected_hint {
        ui.painter().text(
            Pos2::new(select.right() + 6.0, select.center().y),
            Align2::LEFT_CENTER,
            hint,
            FontId::proportional(12.0),
            theme::selected_bg(),
        );
    }
    y += BUTTON_H + GAP;

    // 配置小节标题。
    ui.painter().text(
        Pos2::new(left, y + SECTION_H / 2.0),
        Align2::LEFT_CENTER,
        &model.config_title,
        FontId::proportional(12.0),
        theme::text_on_weak(),
    );
    y += SECTION_H + ROW_GAP;

    for row in &model.rows {
        draw_row(ui, left, right, y, row);
        y += ROW_H + ROW_GAP;
    }

    if let Some(convert) = &model.convert {
        y += ROW_GAP;
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, BUTTON_H));
        if widgets::button(
            ui,
            rect,
            &convert.label,
            ButtonStyle {
                fill: theme::toolbar_bg(),
                hover: theme::accent_strong(),
                text: if convert.enabled {
                    theme::text()
                } else {
                    theme::text_muted()
                },
                enabled: convert.enabled,
            },
        )
        .clicked()
        {
            push_action(ui.ctx(), RightSidebarAction::ConvertClicked);
        }
    }
}

/// 一行「标签 + 右侧控件」；控件右对齐。
fn draw_row(ui: &mut egui::Ui, left: f32, right: f32, y: f32, row: &I2mRowModel) {
    let center = y + ROW_H / 2.0;
    ui.painter().text(
        Pos2::new(left, center),
        Align2::LEFT_CENTER,
        &row.label,
        FontId::proportional(12.0),
        theme::text_muted(),
    );

    match &row.control {
        I2mControlModel::Input { field, text } => {
            let rect =
                Rect::from_min_size(Pos2::new(right - INPUT_W, y), Vec2::new(INPUT_W, ROW_H));
            if let Some(next) = input(ui, rect, *field, text) {
                push_action(
                    ui.ctx(),
                    RightSidebarAction::I2mConfigTextChanged {
                        field: *field,
                        text: next,
                    },
                );
            }
        }
        I2mControlModel::KeyRange {
            start_field,
            start_text,
            end_field,
            end_text,
        } => {
            let tilde_w = widgets::text_width(ui, "~", 12.0);
            let total = INPUT_W * 2.0 + tilde_w + 8.0;
            let mut x = right - total;
            let start = Rect::from_min_size(Pos2::new(x, y), Vec2::new(INPUT_W, ROW_H));
            if let Some(next) = input(ui, start, *start_field, start_text) {
                push_action(
                    ui.ctx(),
                    RightSidebarAction::I2mConfigTextChanged {
                        field: *start_field,
                        text: next,
                    },
                );
            }
            x += INPUT_W + 4.0;
            ui.painter().text(
                Pos2::new(x, center),
                Align2::LEFT_CENTER,
                "~",
                FontId::proportional(12.0),
                theme::text_muted(),
            );
            x += tilde_w + 4.0;
            let end = Rect::from_min_size(Pos2::new(x, y), Vec2::new(INPUT_W, ROW_H));
            if let Some(next) = input(ui, end, *end_field, end_text) {
                push_action(
                    ui.ctx(),
                    RightSidebarAction::I2mConfigTextChanged {
                        field: *end_field,
                        text: next,
                    },
                );
            }
        }
        I2mControlModel::Palette { options, selected } => {
            let rect =
                Rect::from_min_size(Pos2::new(right - COMBO_W, y), Vec2::new(COMBO_W, ROW_H));
            let current = options.get(*selected).cloned().unwrap_or_default();
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            if let Some(index) = combo(&mut child, options, &current) {
                push_action(ui.ctx(), RightSidebarAction::I2mPaletteChanged(index));
            }
        }
    }
}

/// 数字输入框；返回变更后的文本。
fn input(ui: &mut egui::Ui, rect: Rect, field: I2mField, text: &str) -> Option<String> {
    record_rect(ui.ctx(), format!("field:{}", field as u8), rect);
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut value = text.to_owned();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("i2m-input", field as u8), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut value)
                    .frame(egui::Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(FontId::proportional(12.0))
                    .text_color(theme::text())
                    .desired_width(f32::INFINITY),
            )
        })
        .inner;
    response.changed().then_some(value)
}

/// 紧凑下拉（12px 文本、22 高、118 宽、`background.weak` 底）。
fn combo(ui: &mut egui::Ui, options: &[String], selected: &str) -> Option<usize> {
    let mut chosen = None;
    let previous_padding = ui.spacing().button_padding;
    let previous_interact = ui.spacing().interact_size;
    ui.spacing_mut().button_padding = Vec2::new(6.0, 3.0);
    ui.spacing_mut().interact_size.y = ROW_H;
    let previous_visuals = ui.visuals().clone();
    ui.visuals_mut().widgets.inactive.weak_bg_fill = theme::toolbar_bg();
    ui.visuals_mut().widgets.inactive.bg_fill = theme::toolbar_bg();
    ui.visuals_mut().widgets.inactive.fg_stroke.color = theme::text();
    let response = egui::ComboBox::from_id_salt("lumino.right-sidebar.i2m.palette")
        .selected_text(RichText::new(selected).size(12.0).color(theme::text()))
        .width(COMBO_W)
        .show_ui(ui, |ui| {
            for (index, option) in options.iter().enumerate() {
                if ui
                    .selectable_label(
                        option == selected,
                        RichText::new(option.as_str()).size(12.0),
                    )
                    .clicked()
                {
                    chosen = Some(index);
                }
            }
        });
    ui.spacing_mut().button_padding = previous_padding;
    ui.spacing_mut().interact_size = previous_interact;
    *ui.visuals_mut() = previous_visuals;
    ui.painter().rect_stroke(
        response.response.rect,
        CornerRadius::same(4),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    chosen
}
