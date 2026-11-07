//! 底部状态栏。
//!
//! 生产 iced 结构：左侧 12px 描述文本（固定 220px 宽），右侧
//! `CPU / MEM / FPS` 指标组——label 弱色、value 强调色、间隔 12px。

use egui::{Align, Color32, Layout};

use crate::theme;
use crate::widgets;

/// 渲染状态栏。
pub fn show(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::TOOLBAR_BG);

    ui.horizontal_centered(|ui| {
        ui.add_space(8.0);
        widgets::label(ui, "就绪", 12.0, theme::TEXT_MUTED);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            metrics(ui);
        });
    });
}

fn metrics(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.x = 4.0;
    // right_to_left：视觉顺序为 CPU / MEM / FPS。
    metric(ui, "FPS", "50.0");
    ui.add_space(8.0);
    metric(ui, "MEM", "0.0 MB");
    ui.add_space(8.0);
    metric(ui, "CPU", "0.0%");
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    widgets::label(ui, value, 12.0, accent_value());
    ui.spacing_mut().item_spacing.x = 2.0;
    widgets::label(ui, label, 12.0, theme::TEXT_MUTED);
    ui.spacing_mut().item_spacing.x = 4.0;
}

/// 指标数值颜色（`palette.primary.strong`）。
fn accent_value() -> Color32 {
    theme::ACCENT_STRONG
}
