//! 底部状态栏。
//!
//! 生产 iced 结构（`statusbar.rs::view`）：
//! - 高 20，水平内边距 8，行间距 8；
//! - 左侧 12px 描述文本（固定 220 宽，不换行）；
//! - 右侧 `CPU / MEM / FPS` 指标组（label 弱色、value `primary.strong`），
//!   组间额外 12px 间隔，最后是 `right_text`。

use egui::{Align, Color32, Layout, Vec2};

use super::StatusbarModel;
use crate::theme;
use crate::widgets;

/// 左侧描述文本固定宽度（生产 `Length::Fixed(220)`）。
const LEFT_W: f32 = 220.0;
/// 状态栏字号。
const FONT: f32 = 12.0;
/// 指标组之间的额外间隔。
const GROUP_GAP: f32 = 14.0;
/// label 与 value 的间隔（实测 iced 基线：`CPU0.0%` 贴合，仅剩字形侧边距）。
const METRIC_GAP: f32 = 1.0;

/// 草稿场景入口：固定「就绪」与 CPU / MEM / FPS 指标。
pub fn show(ui: &mut egui::Ui) {
    show_model(
        ui,
        &StatusbarModel {
            left: "就绪".to_owned(),
            left_accent: false,
            cpu_percent: Some(0.0),
            memory_mb: 0.0,
            fps: Some(50.0),
            right: String::new(),
        },
    );
}

/// 按真实状态渲染状态栏。
pub fn show_model(ui: &mut egui::Ui, model: &StatusbarModel) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::TOOLBAR_BG);

    // 生产 `padding([0, 8])`：水平 8，垂直 0。
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top()),
        egui::pos2(rect.right() - 8.0, rect.bottom()),
    );
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = METRIC_GAP;

    bar.allocate_ui_with_layout(
        Vec2::new(LEFT_W, inner.height()),
        Layout::left_to_right(Align::Center),
        |ui| {
            let color = if model.left_accent {
                theme::ACCENT_STRONG
            } else {
                theme::TEXT
            };
            ui.add(
                egui::Label::new(egui::RichText::new(&model.left).size(FONT).color(color))
                    .selectable(false)
                    .truncate(),
            );
        },
    );

    bar.with_layout(Layout::right_to_left(Align::Center), |ui| {
        metrics(ui, model);
    });
}

/// 右侧指标组（从右向左排布，视觉顺序 CPU / MEM / FPS / right_text）。
fn metrics(ui: &mut egui::Ui, model: &StatusbarModel) {
    // 生产布局里 `right_text` 恒占一个行位：即使为空串，perf 组右边仍留 8px。
    ui.add_space(8.0);
    if !model.right.is_empty() {
        widgets::label(ui, &model.right, FONT, theme::TEXT);
        ui.add_space(8.0);
    }
    if !model.has_metrics() {
        return;
    }

    if let Some(fps) = model.fps {
        metric(ui, "FPS", &format!("{fps:.1}"));
        ui.add_space(GROUP_GAP);
    }
    metric(ui, "MEM", &format!("{:.1} MB", model.memory_mb));
    ui.add_space(GROUP_GAP);
    if let Some(cpu) = model.cpu_percent {
        metric(ui, "CPU", &format!("{cpu:.1}%"));
    }
}

/// 单个指标：`label` 弱色、`value` 强调色。
fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    widgets::label(ui, value, FONT, accent_value());
    widgets::label(ui, label, FONT, theme::TEXT_MUTED);
}

/// 指标数值颜色（`palette.primary.strong`）。
fn accent_value() -> Color32 {
    theme::ACCENT_STRONG
}
