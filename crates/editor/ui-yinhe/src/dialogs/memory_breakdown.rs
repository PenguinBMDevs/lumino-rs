//! 内存明细对话框 — yinhe `dialogs/memory_breakdown.rs:121` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{column, container, row, scrollable, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 内存快照（展示用子集，对齐 `yinhe_memtrace::Snapshot`）
#[derive(Debug, Clone, Default)]
pub struct MemorySnapshot {
    pub total_mb: f64,
    pub gpu_mb: f64,
    pub rss_mb: f64,
    pub metal_mb: Option<f64>,
    pub by_tag: Vec<(String, f64)>,
    pub enabled: bool,
}

/// 渲染内存明细对话框
pub fn view<'a>(window: &'a Window, snapshot: &'a MemorySnapshot) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let muted = palette.background.weak.text;

    let header: Element<'a> = if snapshot.enabled {
        text(format!("allocator: {:.1} MB", snapshot.total_mb))
            .size(12)
            .into()
    } else {
        text("memory not_enabled")
            .size(12)
            .style(move |_t: &Theme| iced_widget::text::Style { color: Some(muted) })
            .into()
    };

    let rss = text(format!("rss: {:.1} MB", snapshot.rss_mb)).size(12);
    let gpu = text(format!("gpu: {:.1} MB", snapshot.gpu_mb)).size(12);
    let metal: Element<'a> = if let Some(m) = snapshot.metal_mb {
        text(format!("metal: {:.1} MB", m)).size(12).into()
    } else {
        iced_widget::Space::new().height(0).into()
    };

    let tags: Vec<Element<'a>> = if snapshot.enabled {
        snapshot
            .by_tag
            .iter()
            .map(|(name, mb)| {
                row![
                    text(name).size(11),
                    iced_widget::Space::new().width(Length::Fill),
                    text(format!("{mb:.1} MB")).size(11),
                ]
                .into()
            })
            .collect()
    } else {
        Vec::new()
    };

    let body = column![
        header,
        rss,
        gpu,
        metal,
        iced_widget::Space::new().height(6),
        container(text("by_subsystem").size(13)).padding([4, 0]),
        container(scrollable(column(tags).spacing(4)).height(Length::Fixed(200.0))).padding(4),
        text("note: memory stats approximate")
            .size(10)
            .style(move |_t: &Theme| iced_widget::text::Style { color: Some(muted) }),
    ]
    .spacing(6)
    .padding(12);

    container(scrollable(body).height(Length::Fill))
        .width(Length::Fixed(360.0))
        .height(Length::Fixed(400.0))
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
