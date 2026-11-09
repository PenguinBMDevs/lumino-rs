//! 视频导出面板渲染（整块右侧内容区）。
//!
//! 通用行渲染：下拉（ComboBox）/ 滑杆 / 复选 / 文本 / 只读标签 / 按钮行。
//! 几何对齐生产 iced 视图：内边距 24、分区标题 16px、标签列宽按行给定、
//! 控件默认 200 宽、行间距 8、分区之间 16。

use egui::{Align, FontId, RichText, Vec2};

pub use crate::chrome::VideoExportAction;
mod widgets;

use crate::chrome::{VideoExportModel, VideoExportPanelModel, VideoRow, VideoSection};
use crate::theme;
use widgets::{
    button, buttons, label_cell, preview_ui, row_rect, text_area, text_input, text_pair_cell,
    text_width,
};

const ACTION_ID: &str = "lumino.video-export.action";
/// 面板内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 行间距（生产 `column.spacing(8)`）。
const GAP: f32 = 8.0;
/// 分区之间（生产 `space().height(16)`）。
const SECTION_GAP: f32 = 16.0;
/// 控件行高。
const ROW_H: f32 = 33.0;
/// 按钮高度（`padding([6,16])` / `[8,32]`）。
const BUTTON_H: f32 = 34.0;

/// 记录本帧动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, action: VideoExportAction) {
    if ctx
        .data(|d| d.get_temp::<Option<VideoExportAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<VideoExportAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<VideoExportAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整块面板。
pub fn show_panel(ui: &mut egui::Ui, model: &VideoExportPanelModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    let rect = model.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    ui.painter().rect_filled(rect, 0, theme::base_bg());
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(PAD))
            .layout(egui::Layout::top_down(Align::Min)),
    );
    content(&mut root, &model.content);
}

/// 面板内容（滚动区）。
fn content(ui: &mut egui::Ui, model: &VideoExportModel) {
    {
        let scroll = &mut ui.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.fg_stroke.color = theme::selected_bg();
    visuals.widgets.hovered.fg_stroke.color = theme::hover_border();
    visuals.widgets.active.fg_stroke.color = theme::active_bg();

    egui::ScrollArea::vertical()
        .id_salt("lumino.video-export.scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.add(egui::Label::new(
                RichText::new(&model.title).size(18.0).color(theme::text()),
            ));
            ui.add_space(16.0);
            for section in &model.sections {
                section_ui(ui, section);
                ui.add_space(SECTION_GAP);
            }
            if let Some(preview) = &model.preview {
                preview_ui(ui, preview);
                ui.add_space(SECTION_GAP);
            }
            buttons(ui, &model.buttons);
        });
}

/// 一个分区。
fn section_ui(ui: &mut egui::Ui, section: &VideoSection) {
    if !section.title.is_empty() {
        ui.add(egui::Label::new(
            RichText::new(&section.title)
                .size(16.0)
                .color(theme::text()),
        ));
        ui.add_space(12.0);
    }
    let full = ui.available_width();
    for row in &section.rows {
        row_ui(ui, row, full);
        ui.add_space(GAP);
    }
}

/// 一行控件。
fn row_ui(ui: &mut egui::Ui, row: &VideoRow, full: f32) {
    let ctx = ui.ctx().clone();
    match row {
        VideoRow::Pick {
            label,
            label_width,
            options,
            selected,
            width,
            id,
        } => {
            let rect = row_rect(ui, ROW_H);
            label_cell(ui, rect, *label_width, label);
            let combo_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + label_width + GAP, rect.top()),
                Vec2::new(width.min(full - label_width - GAP), ROW_H),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(combo_rect)
                    .layout(egui::Layout::left_to_right(Align::Center)),
            );
            child.spacing_mut().interact_size.y = ROW_H;
            let current = options.get(*selected).map(String::as_str).unwrap_or("");
            let mut chosen = None;
            egui::ComboBox::from_id_salt(("video-pick", *id))
                .selected_text(RichText::new(current).size(14.0))
                .width(combo_rect.width() - 6.0)
                .show_ui(&mut child, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        if ui.selectable_label(index == *selected, option).clicked() {
                            chosen = Some(index);
                        }
                    }
                });
            if let Some(index) = chosen {
                push_action(&ctx, VideoExportAction::Pick { id: *id, index });
            }
        }
        VideoRow::Slider {
            label,
            label_width,
            value,
            min,
            max,
            step,
            slider_width,
            value_text,
            id,
        } => {
            let rect = row_rect(ui, ROW_H);
            label_cell(ui, rect, *label_width, label);
            let slider_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + label_width + GAP, rect.top()),
                Vec2::new(slider_width.min(full - label_width - GAP - 60.0), ROW_H),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(slider_rect)
                    .layout(egui::Layout::left_to_right(Align::Center)),
            );
            child.spacing_mut().interact_size.y = ROW_H;
            let mut v = *value;
            let response = child.add(
                egui::Slider::new(&mut v, *min..=*max)
                    .step_by(*step as f64)
                    .show_value(false),
            );
            if response.changed() {
                push_action(&ctx, VideoExportAction::Slider { id: *id, value: v });
            }
            // 数值用 Label 绘制（进入可访问树，读屏可读）。
            let value_rect = egui::Rect::from_min_max(
                egui::pos2(slider_rect.right() + GAP, rect.top()),
                egui::pos2(rect.right(), rect.bottom()),
            );
            let mut value_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(value_rect)
                    .layout(egui::Layout::left_to_right(Align::Center)),
            );
            value_ui.add(egui::Label::new(
                RichText::new(value_text).size(14.0).color(theme::text()),
            ));
        }
        VideoRow::Toggle { label, checked, id } => {
            let rect = row_rect(ui, ROW_H);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(Align::Center)),
            );
            let mut value = *checked;
            if child
                .checkbox(&mut value, RichText::new(label).size(14.0))
                .changed()
            {
                push_action(&ctx, VideoExportAction::Toggle { id: *id, value });
            }
        }
        VideoRow::Text {
            label,
            label_width,
            value,
            hint,
            width,
            id,
        } => {
            let rect = row_rect(ui, ROW_H);
            let left = if label.is_empty() {
                rect.left()
            } else {
                label_cell(ui, rect, *label_width, label);
                rect.left() + label_width + GAP
            };
            let w = if *width <= 0.0 {
                (rect.right() - left).max(60.0)
            } else {
                width.min(rect.right() - left)
            };
            text_input(
                ui,
                egui::Rect::from_min_size(egui::pos2(left, rect.top()), Vec2::new(w, ROW_H)),
                *id,
                value,
                hint,
            );
        }
        VideoRow::TextButton {
            label,
            label_width,
            value,
            hint,
            width,
            text_id,
            button: tail_button,
        } => {
            let rect = row_rect(ui, ROW_H);
            let left = if label.is_empty() {
                rect.left()
            } else {
                label_cell(ui, rect, *label_width, label);
                rect.left() + label_width + GAP
            };
            let button_w = if tail_button.width > 0.0 {
                tail_button.width
            } else {
                text_width(ui, &tail_button.label, 14.0) + 24.0
            };
            let button_x = rect.right() - button_w;
            let w = if *width <= 0.0 {
                (button_x - GAP - left).max(60.0)
            } else {
                width.min((button_x - GAP - left).max(60.0))
            };
            text_input(
                ui,
                egui::Rect::from_min_size(egui::pos2(left, rect.top()), Vec2::new(w, ROW_H)),
                *text_id,
                value,
                hint,
            );
            if button(
                ui,
                egui::pos2(button_x, rect.top()),
                button_w,
                ROW_H,
                tail_button,
            )
            .clicked()
            {
                push_action(&ctx, VideoExportAction::Button { id: tail_button.id });
            }
        }
        VideoRow::TextArea {
            label,
            value,
            hint,
            height,
            id,
        } => {
            if !label.is_empty() {
                let rect = row_rect(ui, 18.0);
                label_cell(ui, rect, full, label);
            }
            text_area(ui, *height, *id, value, hint);
        }
        VideoRow::TextPair {
            left,
            right,
            separator,
        } => {
            let rect = row_rect(ui, ROW_H);
            let mut x = rect.left();
            for (index, cell) in [left, right].iter().enumerate() {
                if index == 1 {
                    if separator.is_empty() {
                        x += GAP;
                    } else {
                        let w = text_width(ui, separator, 14.0) + GAP;
                        ui.painter().text(
                            egui::pos2(x, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            separator,
                            FontId::proportional(14.0),
                            theme::text(),
                        );
                        x += w;
                    }
                }
                x = text_pair_cell(ui, rect, x, cell);
            }
        }
        VideoRow::Gap { height } => {
            ui.add_space(*height);
        }
        VideoRow::Label { text, size, muted } => {
            ui.add(
                egui::Label::new(RichText::new(text).size(*size).color(if *muted {
                    theme::text_muted()
                } else {
                    theme::text()
                }))
                .wrap(),
            );
        }
        VideoRow::Buttons {
            buttons: row_buttons,
        } => {
            let rect = row_rect(ui, BUTTON_H);
            let mut x = rect.left();
            for button_def in row_buttons {
                let w = if button_def.width > 0.0 {
                    button_def.width
                } else {
                    text_width(ui, &button_def.label, 14.0) + 24.0
                };
                if button(ui, egui::pos2(x, rect.top()), w, BUTTON_H, button_def).clicked() {
                    push_action(&ctx, VideoExportAction::Button { id: button_def.id });
                }
                x += w + GAP;
            }
        }
    }
}
