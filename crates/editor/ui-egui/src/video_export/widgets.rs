//! 视频导出面板的通用控件绘制（预览 / 按钮 / 输入框 / 文本域 / 行度量）。

pub(super) use crate::text::text_width_ceil as text_width;
use egui::{Align, Color32, CornerRadius, FontId, RichText, Stroke, Vec2};

use crate::chrome::{TextPairCell, VideoButton, VideoButtonFill};
use crate::theme;

use super::{BUTTON_H, VideoExportAction, push_action};

/// 底部按钮行。
pub(super) fn buttons(ui: &mut egui::Ui, buttons: &[VideoButton]) {
    let rect = row_rect(ui, BUTTON_H);
    let mut x = rect.left();
    for button_def in buttons {
        let w = if button_def.width > 0.0 {
            button_def.width
        } else {
            text_width(ui, &button_def.label, 14.0) + 48.0
        };
        if button(ui, egui::pos2(x, rect.top()), w, BUTTON_H, button_def).clicked() {
            push_action(
                ui.ctx(),
                VideoExportAction::Button {
                    id: button_def.id.into(),
                },
            );
        }
        x += w + 12.0;
    }
}

/// 行矩形（占位并返回）。
pub(super) fn row_rect(ui: &mut egui::Ui, height: f32) -> egui::Rect {
    let width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
    rect
}

/// 标签单元（左对齐，垂直居中）。
pub(super) fn label_cell(ui: &mut egui::Ui, row: egui::Rect, width: f32, text: &str) {
    if text.is_empty() {
        return;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                row.min,
                Vec2::new(width, row.height()),
            ))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(14.0).color(theme::text())).truncate());
}

/// 固定盒子输入框。
pub(super) fn text_input(ui: &mut egui::Ui, rect: egui::Rect, id: u16, value: &str, hint: &str) {
    let mut buffer = value.to_owned();
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(10.0, 6.0)))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("video-input", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(14.0))
                    .text_color(theme::text())
                    .hint_text(RichText::new(hint).size(14.0).color(theme::text_dim()))
                    .desired_width(rect.width() - 22.0),
            )
        })
        .inner;
    if response.changed() {
        push_action(
            ui.ctx(),
            VideoExportAction::Text {
                id: id.into(),
                value: buffer,
            },
        );
    }
}

/// 按钮。
pub(super) fn button(
    ui: &mut egui::Ui,
    pos: egui::Pos2,
    width: f32,
    height: f32,
    button_def: &VideoButton,
) -> egui::Response {
    let (fill, text) = match button_def.fill {
        VideoButtonFill::Primary => (theme::accent(), theme::on_accent()),
        VideoButtonFill::Neutral => (theme::toolbar_bg(), theme::text()),
        VideoButtonFill::Danger => (theme::danger(), Color32::WHITE),
    };
    let widget =
        crate::widgets::text_button(RichText::new(&button_def.label).size(14.0).color(text))
            .fill(fill)
            .corner_radius(CornerRadius::same(4));
    ui.put(
        egui::Rect::from_min_size(pos, Vec2::new(width, height)),
        widget,
    )
}

/// 一行两列中的一列（标签 + 输入）。
pub(super) fn text_pair_cell(
    ui: &mut egui::Ui,
    row: egui::Rect,
    x: f32,
    cell: &TextPairCell,
) -> f32 {
    let mut x = x;
    if !cell.label.is_empty() {
        let mut label = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    egui::pos2(x, row.top()),
                    Vec2::new(cell.label_width, row.height()),
                ))
                .layout(egui::Layout::left_to_right(Align::Center)),
        );
        label.add(
            egui::Label::new(RichText::new(&cell.label).size(13.0).color(theme::text())).truncate(),
        );
        x += cell.label_width + 6.0;
    }
    let w = cell.width.max(40.0);
    text_input(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, row.top()), Vec2::new(w, row.height())),
        cell.id,
        &cell.value,
        &cell.hint,
    );
    x + w
}

/// 多行文本域。
pub(super) fn text_area(ui: &mut egui::Ui, height: f32, id: u16, value: &str, hint: &str) {
    let rect = row_rect(ui, height);
    let mut buffer = value.to_owned();
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(8.0))
            .layout(egui::Layout::top_down(Align::Min)),
    );
    let response = child
        .push_id(("video-textarea", id), |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::monospace(12.0))
                    .text_color(theme::text())
                    .hint_text(RichText::new(hint).size(12.0).color(theme::text_dim()))
                    .desired_width(rect.width() - 16.0)
                    .desired_rows(6),
            )
        })
        .inner;
    if response.changed() {
        push_action(
            ui.ctx(),
            VideoExportAction::Text {
                id: id.into(),
                value: buffer,
            },
        );
    }
}
