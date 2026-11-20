//! 音频导出：区块内复用的控件（标签 / 输入框 / 下拉 / 复选框 / 说明）。

use egui::{Align, Color32, CornerRadius, FontId, Layout, RichText, Stroke, Vec2};

use crate::chrome::{AudioChoice, AudioToggle};
use crate::theme;

/// 标签列宽度（生产 `text(width(120))`）。
pub(super) const LABEL_W: f32 = 120.0;
/// 输入框 / 下拉宽度（生产 `Length::Fixed(200)`）。
pub(super) const FIELD_W: f32 = 200.0;
/// 控件行高（`text_input.padding([6,10])` + 14px）。
pub(super) const FIELD_H: f32 = 33.0;

/// 一行「标签 + 控件」：`label` 固定宽、控件紧跟（8px 间距）。
pub(super) fn row(ui: &mut egui::Ui, label: &str, label_w: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(label_w, FIELD_H), egui::Sense::hover());
        let mut left = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        left.add(
            egui::Label::new(
                RichText::new(label)
                    .size(theme::font::LABEL)
                    .color(theme::text()),
            )
            .truncate(),
        );
        ui.add_space(8.0);
        add(ui);
    });
}

/// 说明文字（11px，弱色）。
pub(super) fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .size(theme::font::SMALL)
                .color(theme::text_dim()),
        )
        .wrap(),
    );
}

/// 区块标题（16px）。
pub(super) fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(
        RichText::new(text)
            .size(theme::font::SECTION)
            .color(theme::text()),
    ));
}

/// 文本输入框（`base` 底 + 1px `strong` 边框、圆角 4、内边距 6/10）。
pub(super) fn input(
    ui: &mut egui::Ui,
    id: &'static str,
    width: f32,
    value: &str,
    placeholder: &str,
    changed: impl FnOnce(String),
) {
    let mut buffer = value.to_owned();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, FIELD_H), egui::Sense::hover());
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
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("audio-input", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(theme::font::LABEL))
                    .text_color(theme::text())
                    .hint_text(
                        RichText::new(placeholder)
                            .size(theme::font::LABEL)
                            .color(theme::text_dim()),
                    )
                    .desired_width(width - 22.0),
            )
        })
        .inner;
    if response.changed() {
        changed(buffer);
    }
}

/// 只读路径框（`base` 底 + 1px 边框、12px 弱色文本）。
pub(super) fn readonly(ui: &mut egui::Ui, width: f32, value: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, FIELD_H), egui::Sense::hover());
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
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(
        egui::Label::new(
            RichText::new(value)
                .size(theme::font::SMALL)
                .color(theme::text_muted()),
        )
        .truncate(),
    );
}

/// 下拉：`width` 宽、选项按固定表下标回传。
pub(super) fn combo(
    ui: &mut egui::Ui,
    id: &'static str,
    width: f32,
    choices: &[AudioChoice],
    picked: impl FnOnce(usize),
) {
    let selected = choices
        .iter()
        .find(|choice| choice.selected)
        .map(|choice| choice.label.as_str())
        .unwrap_or("");
    let mut chosen = None;
    egui::ComboBox::from_id_salt(("audio-combo", id))
        .selected_text(RichText::new(selected).size(theme::font::LABEL))
        .width(width - 6.0)
        .truncate()
        .show_ui(ui, |ui| {
            for choice in choices {
                if ui
                    .selectable_label(choice.selected, &choice.label)
                    .clicked()
                {
                    chosen = Some(choice.index);
                }
            }
        });
    if let Some(index) = chosen {
        picked(index);
    }
}

/// 开关行：标签在左、开关贴行右缘（替代旧勾选框）。
pub(super) fn toggle(ui: &mut egui::Ui, toggle: &AudioToggle) -> bool {
    let width = ui.available_width().max(160.0);
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), egui::Sense::hover());
    let hit = crate::widgets::switch_row(
        ui,
        row,
        &crate::widgets::SwitchRow {
            id: egui::Id::unique(("audio-export-toggle", toggle.id as u8)),
            checked: toggle.checked,
            enabled: toggle.enabled,
            label: &toggle.label,
            hint: "",
        },
        &theme::switch_style(),
    );
    hit.toggled
}

/// 次要按钮（浏览 / 关闭 / 重置等）：`padding([6,16])`。
pub(super) fn button(ui: &mut egui::Ui, label: &str, width: Option<f32>) -> egui::Response {
    let text_w = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(theme::font::LABEL),
            theme::text(),
        )
        .size()
        .x
        .ceil();
    let w = width.unwrap_or(text_w + 32.0);
    ui.add_sized(
        Vec2::new(w, 30.0),
        crate::widgets::text_button(
            RichText::new(label)
                .size(theme::font::LABEL)
                .color(theme::text()),
        )
        .fill(theme::toolbar_bg())
        .corner_radius(CornerRadius::same(4)),
    )
}

/// 主色按钮（导出）：固定宽度、34 高。
pub(super) fn primary_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    ui.add_sized(
        Vec2::new(width, 34.0),
        crate::widgets::text_button(
            RichText::new(label)
                .size(theme::font::LABEL)
                .color(theme::on_accent()),
        )
        .fill(theme::accent())
        .corner_radius(CornerRadius::same(4)),
    )
}

/// 危险按钮（中止）：红底白字。
pub(super) fn danger_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    ui.add_sized(
        Vec2::new(width, 34.0),
        crate::widgets::text_button(
            RichText::new(label)
                .size(theme::font::LABEL)
                .color(Color32::WHITE),
        )
        .fill(theme::danger())
        .corner_radius(CornerRadius::same(4)),
    )
}
