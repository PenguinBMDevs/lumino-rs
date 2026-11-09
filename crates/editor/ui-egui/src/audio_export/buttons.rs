//! 音频导出：底部按钮区（未渲染 / 渲染中 / 完成三态）。

use super::AudioExportAction;
use super::fields;
use crate::chrome::{AudioButtonsMode, AudioButtonsModel, ExportMessageKind};
use crate::theme;

/// 绘制按钮区。
pub(super) fn show(ui: &mut egui::Ui, model: &AudioButtonsModel) {
    match &model.mode {
        AudioButtonsMode::Actions { close, export } => {
            ui.horizontal(|ui| {
                if fields::button(ui, close, Some(100.0)).clicked() {
                    super::push_action(ui.ctx(), AudioExportAction::ClosePanel);
                }
                ui.add_space(12.0);
                if fields::primary_button(ui, export, 100.0).clicked() {
                    super::push_action(ui.ctx(), AudioExportAction::Confirm);
                }
            });
        }
        AudioButtonsMode::Rendering {
            status,
            progress,
            percentage,
            pause,
            abort,
        } => {
            ui.add(egui::Label::new(
                egui::RichText::new(status)
                    .size(theme::font::LABEL)
                    .color(theme::text()),
            ));
            ui.add_space(8.0);
            progress_bar(ui, *progress);
            ui.add_space(4.0);
            ui.add(egui::Label::new(
                egui::RichText::new(percentage)
                    .size(theme::font::SMALL)
                    .color(theme::text()),
            ));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if fields::button(ui, pause, Some(90.0)).clicked() {
                    super::push_action(ui.ctx(), AudioExportAction::TogglePause);
                }
                ui.add_space(12.0);
                if fields::danger_button(ui, abort, 90.0).clicked() {
                    super::push_action(ui.ctx(), AudioExportAction::Abort);
                }
            });
        }
        AudioButtonsMode::Finished {
            status,
            kind,
            reset,
            close,
        } => {
            let color = match kind {
                ExportMessageKind::Normal => theme::text(),
                ExportMessageKind::Error => theme::danger(),
                ExportMessageKind::Completed => theme::success(),
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(status)
                        .size(theme::font::LABEL)
                        .color(color),
                )
                .wrap(),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if fields::button(ui, reset, Some(90.0)).clicked() {
                    super::push_action(ui.ctx(), AudioExportAction::ResetRendering);
                }
                ui.add_space(12.0);
                if fields::button(ui, close, Some(90.0)).clicked() {
                    super::push_action(ui.ctx(), AudioExportAction::ClosePanel);
                }
            });
        }
    }
}

/// 30 高进度条（轨道 `strong`、进度 `primary.base`、圆角 2）。
fn progress_bar(ui: &mut egui::Ui, ratio: f32) {
    let width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(2), theme::selected_bg());
    let ratio = ratio.clamp(0.0, 1.0);
    if ratio > 0.0 {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * ratio, rect.height())),
            egui::CornerRadius::same(2),
            theme::accent(),
        );
    }
}
