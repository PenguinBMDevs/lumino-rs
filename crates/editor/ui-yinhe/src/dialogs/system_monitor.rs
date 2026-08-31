//! 系统监控面板 — yinhe `dialogs/system_monitor.rs:41` + `memory_breakdown` 互补的 iced 桩
//!
//! 原 `egui` 实现用 `sysinfo` 拉取 CPU/MEM；iced 桩仅保留视图骨架，数据由 Host 注入。

use iced_core::Length;
use iced_widget::{column, container, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 系统监控快照（由 Host 每帧注入）
#[derive(Debug, Clone, Default)]
pub struct SystemMonitorSnapshot {
    pub cpu_usage: f32,
    pub mem_mb: f64,
}

pub fn view<'a>(window: &'a Window, snap: &'a SystemMonitorSnapshot) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        text(format!("CPU {:.1}%", snap.cpu_usage)).size(13),
        text(format!("MEM {:.1} MB", snap.mem_mb)).size(13),
        text("sysinfo refresh via Host")
            .size(10)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }),
    ]
    .spacing(8)
    .padding(16);

    container(content)
        .width(Length::Fixed(360.0))
        .height(Length::Fixed(240.0))
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
