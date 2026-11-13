//! 颜料桶「分音符填充」面板（从 `panels.rs` 拆出，REF-001 行数纪律）。

use egui::{Align2, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::super::FillDivisionPanelModel;
use super::super::super::toolbar_popup::{self, ToolbarAction};
use super::{BTN_H, ERROR, HINT_H, INPUT_H, TEXT, TEXT_DIM, text_width};
use crate::theme;

/// 分音符填充面板：`使用 [ x ] 分音符填充` + 说明 + 三个按钮。
pub(super) fn show(ui: &mut egui::Ui, rect: Rect, model: &FillDivisionPanelModel) {
    let content = Rect::from_min_size(
        Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
        Vec2::new(rect.width() - 20.0, rect.height() - 20.0),
    );
    let mut y = content.top();

    // 输入行：前缀 + 输入框 + 后缀（整体居中）。
    let prefix_w = text_width(ui, &model.prefix, 14.0);
    let suffix_w = text_width(ui, &model.suffix, 14.0);
    let row_w = prefix_w + 6.0 + 72.0 + 6.0 + suffix_w;
    let mut x = content.center().x - row_w / 2.0;
    ui.painter().text(
        Pos2::new(x, y + INPUT_H / 2.0),
        Align2::LEFT_CENTER,
        &model.prefix,
        FontId::proportional(14.0),
        TEXT,
    );
    x += prefix_w + 6.0;
    let input = Rect::from_min_size(Pos2::new(x, y), Vec2::new(72.0, INPUT_H));
    text_input(ui, input, &model.value);
    x += 72.0 + 6.0;
    ui.painter().text(
        Pos2::new(x, y + INPUT_H / 2.0),
        Align2::LEFT_CENTER,
        &model.suffix,
        FontId::proportional(14.0),
        TEXT,
    );
    y += INPUT_H + 4.0;

    ui.painter().text(
        Pos2::new(content.center().x, y + HINT_H / 2.0),
        Align2::CENTER_CENTER,
        &model.hint,
        FontId::proportional(11.0),
        TEXT_DIM,
    );
    y += HINT_H;
    if let Some(error) = &model.error {
        ui.painter().text(
            Pos2::new(content.center().x, y + 6.0),
            Align2::CENTER_CENTER,
            error,
            FontId::proportional(10.0),
            ERROR,
        );
        y += 12.0;
    }
    y += 8.0;

    // 按钮行：关闭填充 / 取消 / 确定。
    let labels = [&model.disable, &model.cancel, &model.ok];
    let widths: Vec<f32> = labels
        .iter()
        .map(|label| text_width(ui, label, 13.0) + 28.0)
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 8.0 * (labels.len() as f32 - 1.0);
    let mut x = content.center().x - total / 2.0;
    for (index, label) in labels.iter().enumerate() {
        let button = Rect::from_min_size(Pos2::new(x, y), Vec2::new(widths[index], BTN_H));
        let response = ui.interact(
            button,
            egui::Id::unique(("lumino.fill-button", index)),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter().rect_filled(
                button,
                CornerRadius::same(4),
                egui::Color32::from_white_alpha(26),
            );
        }
        ui.painter().text(
            button.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.0),
            TEXT,
        );
        if response.clicked() {
            let action = match index {
                0 => ToolbarAction::FillDisable,
                1 => ToolbarAction::CloseFillPanel,
                _ => ToolbarAction::FillDivisionConfirm,
            };
            toolbar_popup::push_action(ui.ctx(), action);
        }
        x += widths[index] + 8.0;
    }
}

/// 分音符填充输入框（数字/文本均可，校验由生产负责）。
fn text_input(ui: &mut egui::Ui, rect: Rect, value: &str) {
    super::super::set_fill_input_rect(ui.ctx(), rect);
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    let mut text = value.to_owned();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let response = child.add(egui::TextEdit::singleline(&mut text).frame(egui::Frame::NONE));
    if response.hovered() {
        tracing::debug!("填充输入框悬停：rect={:?}", response.rect);
    }
    if response.clicked() {
        tracing::debug!("填充输入框被点击：has_focus={}", response.has_focus());
    }
    if response.changed() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::FillDivisionChanged(text));
    }
}
