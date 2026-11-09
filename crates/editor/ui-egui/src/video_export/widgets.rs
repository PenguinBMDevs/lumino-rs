//! 视频导出面板的通用控件绘制（预览 / 按钮 / 输入框 / 文本域 / 行度量）。

use egui::{Align, Color32, CornerRadius, FontId, RichText, Stroke, Vec2};

use crate::chrome::{TextPairCell, VideoButton, VideoButtonFill};
use crate::theme;

use super::{BUTTON_H, VideoExportAction, push_action};

/// 预览区（空态：weak 底 + 1px 边框，120 高居中）。
pub(super) fn preview_ui(ui: &mut egui::Ui, preview: &crate::chrome::VideoPreviewModel) {
    let rect = row_rect(ui, 120.0);
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    match preview.image_size {
        Some((w, h)) => {
            let (dw, dh) = fit_preview(w, h);
            let img = egui::Rect::from_center_size(rect.center(), Vec2::new(dw, dh));
            ui.painter()
                .rect_filled(img, CornerRadius::same(4), theme::base_bg());
        }
        None => {
            // 空态文案用 Label（进入可访问树）。
            let mut empty = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            ));
            empty.add(egui::Label::new(
                RichText::new(&preview.label)
                    .size(14.0)
                    .color(theme::text_muted()),
            ));
        }
    }
}

/// 预览图等比缩放到 480×240 以内（与原实现同）。
pub(super) fn fit_preview(w: f32, h: f32) -> (f32, f32) {
    let scale = (480.0 / w).min(240.0 / h).min(1.0);
    ((w * scale).max(100.0), (h * scale).max(56.0))
}

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
            push_action(ui.ctx(), VideoExportAction::Button { id: button_def.id });
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
    child.add(egui::Label::new(
        RichText::new(text).size(14.0).color(theme::text()),
    ));
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
        push_action(ui.ctx(), VideoExportAction::Text { id, value: buffer });
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
    let widget = egui::Button::new(RichText::new(&button_def.label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(
        egui::Rect::from_min_size(pos, Vec2::new(width, height)),
        widget,
    )
}

pub(super) fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::text())
        .size()
        .x
        .ceil()
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
        label.add(egui::Label::new(
            RichText::new(&cell.label).size(13.0).color(theme::text()),
        ));
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
        push_action(ui.ctx(), VideoExportAction::Text { id, value: buffer });
    }
}
