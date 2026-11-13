//! 音频导出：音频设置区块。

use super::AudioExportAction;
use super::fields::{self, FIELD_W, LABEL_W};
use crate::chrome::AudioSettingsModel;

/// 绘制音频设置区块。
pub(super) fn show(ui: &mut egui::Ui, model: &AudioSettingsModel) {
    let ctx = ui.ctx().clone();
    fields::section_title(ui, &model.title);
    ui.add_space(12.0);

    fields::row(ui, &model.backend_label, LABEL_W, |ui| {
        fields::combo(ui, "backend", FIELD_W, &model.backends, |index| {
            super::push_action(&ctx, AudioExportAction::BackendChanged(index))
        });
    });
    ui.add_space(4.0);
    fields::hint(ui, &model.backend_hint);
    ui.add_space(12.0);

    fields::row(ui, &model.format_label, LABEL_W, |ui| {
        fields::combo(ui, "format", FIELD_W, &model.formats, |index| {
            super::push_action(&ctx, AudioExportAction::FormatChanged(index))
        });
    });
    ui.add_space(4.0);
    fields::hint(ui, &model.format_hint);
    ui.add_space(8.0);

    fields::row(ui, &model.bitrate_label, LABEL_W, |ui| {
        fields::input(ui, "bitrate", FIELD_W, &model.bitrate, "320", |text| {
            super::push_action(&ctx, AudioExportAction::BitrateChanged(text))
        });
    });
    ui.add_space(4.0);
    fields::hint(ui, &model.bitrate_hint);
    ui.add_space(8.0);

    fields::row(ui, &model.sample_rate_label, LABEL_W, |ui| {
        fields::combo(ui, "sample-rate", FIELD_W, &model.sample_rates, |index| {
            super::push_action(&ctx, AudioExportAction::SampleRateChanged(index))
        });
    });
    ui.add_space(4.0);
    fields::hint(ui, &model.sample_rate_hint);
    ui.add_space(8.0);

    fields::row(ui, &model.channels_label, LABEL_W, |ui| {
        fields::combo(ui, "channels", FIELD_W, &model.channels, |index| {
            super::push_action(&ctx, AudioExportAction::ChannelsChanged(index))
        });
    });
    ui.add_space(8.0);

    fields::row(ui, &model.layers_label, LABEL_W, |ui| {
        fields::input(ui, "layers", FIELD_W, &model.layers, "32", |text| {
            super::push_action(&ctx, AudioExportAction::LayersChanged(text))
        });
    });
    ui.add_space(4.0);
    fields::hint(ui, &model.layers_hint);
    if let Some(warning) = &model.layers_warning {
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(warning)
                    .size(crate::theme::font::SMALL)
                    .color(crate::theme::danger()),
            )
            .wrap(),
        );
    }
    ui.add_space(8.0);

    fields::row(ui, &model.channel_thread_label, LABEL_W, |ui| {
        fields::combo(
            ui,
            "channel-threading",
            FIELD_W,
            &model.channel_threads,
            |index| super::push_action(&ctx, AudioExportAction::ChannelThreadingChanged(index)),
        );
    });
    ui.add_space(8.0);
    fields::row(ui, &model.key_thread_label, LABEL_W, |ui| {
        fields::combo(ui, "key-threading", FIELD_W, &model.key_threads, |index| {
            super::push_action(&ctx, AudioExportAction::KeyThreadingChanged(index))
        });
    });
    ui.add_space(8.0);

    fields::row(ui, &model.interpolation_label, LABEL_W, |ui| {
        fields::combo(
            ui,
            "interpolation",
            FIELD_W,
            &model.interpolations,
            |index| super::push_action(&ctx, AudioExportAction::InterpolationChanged(index)),
        );
    });
    ui.add_space(4.0);
    fields::hint(ui, &model.interpolation_hint);
    ui.add_space(12.0);

    for toggle in &model.toggles {
        if fields::toggle(ui, toggle) {
            super::push_action(&ctx, AudioExportAction::Toggle(toggle.id, !toggle.checked));
        }
        ui.add_space(4.0);
    }
}
