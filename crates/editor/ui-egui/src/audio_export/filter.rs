//! 音频导出：事件过滤区块。

use super::fields::{self, LABEL_W};
use super::{AudioExportAction, AudioToggleId};
use crate::chrome::AudioFilterModel;

/// 绘制事件过滤区块。
pub(super) fn show(ui: &mut egui::Ui, model: &AudioFilterModel) {
    let ctx = ui.ctx().clone();
    fields::section_title(ui, &model.title);
    ui.add_space(8.0);

    if fields::toggle(ui, &model.ignore) {
        super::push_action(
            &ctx,
            AudioExportAction::Toggle(AudioToggleId::IgnoreProgram, !model.ignore.checked),
        );
    }
    fields::hint(ui, &model.ignore_hint);
    ui.add_space(8.0);

    ui.add(
        egui::Label::new(
            egui::RichText::new(&model.velocity_title)
                .size(crate::theme::font::LABEL)
                .color(crate::theme::text()),
        )
        .wrap(),
    );
    ui.add_space(4.0);
    if fields::toggle(ui, &model.velocity_toggle) {
        super::push_action(
            &ctx,
            AudioExportAction::Toggle(
                AudioToggleId::FilterVelocity,
                !model.velocity_toggle.checked,
            ),
        );
    }
    ui.add_space(4.0);
    range_row(
        ui,
        &ctx,
        &model.velocity_range_label,
        &model.velocity_low,
        &model.velocity_high,
        AudioExportAction::VelocityLowChanged,
        AudioExportAction::VelocityHighChanged,
    );
    ui.add_space(8.0);

    ui.add(
        egui::Label::new(
            egui::RichText::new(&model.key_title)
                .size(crate::theme::font::LABEL)
                .color(crate::theme::text()),
        )
        .wrap(),
    );
    ui.add_space(4.0);
    if fields::toggle(ui, &model.key_toggle) {
        super::push_action(
            &ctx,
            AudioExportAction::Toggle(AudioToggleId::FilterKey, !model.key_toggle.checked),
        );
    }
    ui.add_space(4.0);
    range_row(
        ui,
        &ctx,
        &model.key_range_label,
        &model.key_low,
        &model.key_high,
        AudioExportAction::KeyLowChanged,
        AudioExportAction::KeyHighChanged,
    );
    ui.add_space(8.0);

    fields::row(ui, &model.delay_label, 140.0, |ui| {
        fields::input(ui, "note-delay", 180.0, &model.delay, "0", |text| {
            super::push_action(&ctx, AudioExportAction::NoteForceEndDelayChanged(text))
        });
    });
    ui.add_space(4.0);
    fields::hint(ui, &model.delay_hint);
}

/// 范围行：标签 120 + 80 输入 + 「 ~ 」+ 80 输入。
fn range_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    label: &str,
    low: &str,
    high: &str,
    low_action: fn(String) -> AudioExportAction,
    high_action: fn(String) -> AudioExportAction,
) {
    fields::row(ui, label, LABEL_W, |ui| {
        fields::input(ui, "range-low", 80.0, low, "0", |text| {
            super::push_action(ctx, low_action(text))
        });
        ui.add_space(4.0);
        ui.add(egui::Label::new(
            egui::RichText::new(" ~ ")
                .size(crate::theme::font::LABEL)
                .color(crate::theme::text()),
        ));
        ui.add_space(4.0);
        fields::input(ui, "range-high", 80.0, high, "127", |text| {
            super::push_action(ctx, high_action(text))
        });
    });
}
