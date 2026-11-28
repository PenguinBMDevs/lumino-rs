//! 形状抽屉的控件原语：按钮、输入框、固定矩形文本。

use crate::theme;
use egui::{Align, Color32, CornerRadius, FontId, RichText, Stroke, Vec2};

use super::action::{ShapeDrawerAction, push_action};

/// 面板按钮（`padding([2,6])` 口径）。
pub(super) fn button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    let button = crate::widgets::text_button(RichText::new(label).size(16.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(3));
    ui.put(rect, button)
}

/// 固定盒子输入框。
pub(super) fn input(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &'static str,
    value: &str,
    placeholder: &str,
    changed: impl FnOnce(String),
) {
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
            .max_rect(rect.shrink2(Vec2::new(6.0, 5.0)))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("shape-input", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(14.0))
                    .text_color(theme::text())
                    .hint_text(
                        RichText::new(placeholder)
                            .size(14.0)
                            .color(theme::text_dim()),
                    )
                    .desired_width((rect.width() - 14.0).max(10.0)),
            )
        })
        .inner;
    if response.changed() {
        changed(buffer);
    }
}

/// 记录面板动作。
pub(super) fn act(ctx: &egui::Context, action: crate::chrome::ShapePanelAction) {
    push_action(ctx, ShapeDrawerAction::Panel(action));
}

/// 固定矩形内的左对齐文本（保留可访问树节点）。
pub(super) fn label_left(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    text: &str,
    size: f32,
    color: Color32,
) {
    label_left_in(ui, rect, text, size, color);
}

pub(super) fn label_left_in(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    text: &str,
    size: f32,
    color: Color32,
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .selectable(false)
            .truncate(),
    );
}
