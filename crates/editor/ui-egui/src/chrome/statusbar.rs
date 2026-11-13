//! 底部状态栏。
//!
//! 生产 旧版 结构（`statusbar.rs::view`）：
//! - 高 20，水平内边距 8，行间距 8；
//! - 左侧 12px 描述文本（固定 220 宽，不换行）；
//! - 右侧 `CPU / MEM / FPS` 指标组（label 弱色、value `primary.strong`），
//!   组间额外 12px 间隔，最后是 `right_text`。

use egui::{Align, Color32, Layout, Response, Vec2};

use super::{GpuMetricsModel, MetricTone, StatusbarModel, governor_tone, load_tone, underrun_tone};
use crate::theme;
use crate::widgets;

/// 左侧描述文本固定宽度（生产 `Length::Fixed(220)`）。
const LEFT_W: f32 = 220.0;
/// 状态栏字号。
const FONT: f32 = 12.0;
/// 指标组之间的额外间隔。
const GROUP_GAP: f32 = 14.0;
/// label 与 value 的间隔（实测 旧版 基线：`CPU0.0%` 贴合，仅剩字形侧边距）。
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
            gpu: None,
        },
    );
}

/// 按真实状态渲染状态栏。
pub fn show_model(ui: &mut egui::Ui, model: &StatusbarModel) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::toolbar_bg());

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
                theme::accent_strong()
            } else {
                theme::text()
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
        widgets::label(ui, &model.right, FONT, theme::text());
        ui.add_space(8.0);
    }
    if !model.has_metrics() {
        return;
    }

    if let Some(fps) = model.fps {
        metric(ui, "FPS", &format!("{fps:.1}"), MetricTone::Normal);
        ui.add_space(GROUP_GAP);
    }
    if metric(
        ui,
        "MEM",
        &format!("{:.1} MB", model.memory_mb),
        MetricTone::Normal,
    )
    .clicked()
    {
        push_action(ui.ctx(), StatusbarAction::OpenMemoryMonitor);
    }
    ui.add_space(GROUP_GAP);
    if let Some(cpu) = model.cpu_percent {
        metric(ui, "CPU", &format!("{cpu:.1}%"), MetricTone::Normal);
    }
    // GPU 组在 CPU/MEM/FPS 左侧，中间固定 12px（旧版 `space().width(12)`）。
    if let Some(gpu) = &model.gpu {
        ui.add_space(12.0);
        gpu_group(ui, gpu);
    }
}

/// 单个指标：`label` 弱色、`value` 强调色。返回数值文本的响应（供可点击指标使用）。
fn metric(ui: &mut egui::Ui, label: &str, value: &str, tone: MetricTone) -> Response {
    // 右侧栏是「从右向左」布局：先加数值（靠右），再加标签（靠左）。
    // `Label` 默认不吃指针事件，必须显式 `Sense::click()`：MEM 数值是
    // 「打开内存监控」的入口（与 旧版 `metric_clickable_value` 同义）。
    let response = ui.add(
        egui::Label::new(
            egui::RichText::new(value)
                .size(FONT)
                .color(tone_color(tone)),
        )
        .selectable(false)
        .sense(egui::Sense::click()),
    );
    widgets::label(ui, label, FONT, theme::text_muted());
    response
}

/// 色调 → 颜色（与 旧版 `metric_value_toned` 同源）。
fn tone_color(tone: MetricTone) -> Color32 {
    match tone {
        MetricTone::Normal => accent_value(),
        MetricTone::Warning => theme::warning(),
        MetricTone::Danger => theme::danger(),
    }
}

/// GPU (LGS) 指标组：`GPU Q …ms<趋势>` / LOAD / V / UR / GMEM / CMEM / GOV / DROP。
///
/// 父布局是 right-to-left（整组排在 CPU/MEM/FPS 左侧），子项在 rtl 容器里
/// **先加的在右**，因此这里按视觉顺序**倒序**添加，才能得到与 旧版 一致的
/// 从左到右排列。
fn gpu_group(ui: &mut egui::Ui, gpu: &GpuMetricsModel) {
    ui.spacing_mut().item_spacing.x = 6.0;
    if let Some(dropped) = gpu.dropped {
        metric(ui, "DROP", &dropped.to_string(), MetricTone::Danger);
        ui.add_space(8.0);
    }
    if let Some(level) = gpu.governor_level {
        metric(ui, "GOV", &level.to_string(), governor_tone(level));
        ui.add_space(8.0);
    }
    if let Some(text) = &gpu.sample_cpu {
        metric(ui, "CMEM", text, MetricTone::Normal);
        ui.add_space(8.0);
    }
    if let Some(text) = &gpu.sample_gpu {
        metric(ui, "GMEM", text, MetricTone::Normal);
        ui.add_space(8.0);
    }
    metric(
        ui,
        "UR",
        &gpu.underruns.to_string(),
        underrun_tone(gpu.underruns),
    );
    ui.add_space(8.0);
    metric(ui, "V", &gpu.voice_count.to_string(), MetricTone::Normal);
    ui.add_space(8.0);
    metric(
        ui,
        "LOAD",
        &format!("{:.0}%", gpu.load * 100.0),
        load_tone(gpu.load),
    );
    ui.add_space(8.0);
    metric(
        ui,
        "GPU",
        &format!("Q {:.0}ms{}", gpu.queue_ms, gpu.trend.arrow()),
        MetricTone::Normal,
    );
}

/// 状态栏动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusbarAction {
    /// 点击 MEM 数值 → 打开内存监控对话框。
    OpenMemoryMonitor,
}

/// 动作通道（一帧只保留第一个）。
const ACTION_ID: &str = "lumino.statusbar.action";

/// 记录本帧动作。
pub fn push_action(ctx: &egui::Context, action: StatusbarAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<StatusbarAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<StatusbarAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<StatusbarAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 指标数值颜色（`palette.primary.strong`）。
fn accent_value() -> Color32 {
    theme::accent_strong()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_thresholds_match_legacy() {
        assert_eq!(load_tone(0.0), MetricTone::Normal);
        assert_eq!(load_tone(0.59), MetricTone::Normal);
        assert_eq!(load_tone(0.6), MetricTone::Warning);
        assert_eq!(load_tone(0.89), MetricTone::Warning);
        assert_eq!(load_tone(0.9), MetricTone::Danger);
        assert_eq!(load_tone(2.5), MetricTone::Danger, ">100% 仍为危险档");

        assert_eq!(underrun_tone(0), MetricTone::Normal);
        assert_eq!(underrun_tone(3), MetricTone::Danger);

        assert_eq!(governor_tone(0), MetricTone::Normal);
        assert_eq!(governor_tone(1), MetricTone::Warning);
        assert_eq!(governor_tone(2), MetricTone::Danger);
        assert_eq!(governor_tone(3), MetricTone::Danger);
    }

    #[test]
    fn queue_trend_arrows_match_legacy() {
        assert_eq!(super::super::GpuQueueTrend::Up.arrow(), "↑");
        assert_eq!(super::super::GpuQueueTrend::Down.arrow(), "↓");
        assert_eq!(super::super::GpuQueueTrend::Flat.arrow(), "→");
    }
}
