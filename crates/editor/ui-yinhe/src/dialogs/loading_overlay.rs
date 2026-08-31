//! 加载遮罩对话框 — yinhe `dialogs/loading_overlay.rs:111` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{column, container, progress_bar, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    Done,
    Active,
    Pending,
}

#[derive(Debug, Clone)]
pub struct StageProgress {
    pub label: String,
    pub detail: String,
    pub progress: f32,
    pub status: StageStatus,
}

/// 渲染加载遮罩
pub fn view<'a>(window: &'a Window, stages: &'a [StageProgress]) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let stage_rows: Vec<Element<'a>> = stages
        .iter()
        .map(|s| {
            let icon = match s.status {
                StageStatus::Done => "✓",
                StageStatus::Active => "⟳",
                StageStatus::Pending => "○",
            };
            let detail: Element<'a> = if s.detail.is_empty() {
                iced_widget::Space::new().height(0).into()
            } else {
                text(&s.detail)
                    .size(10)
                    .style(move |_t: &Theme| iced_widget::text::Style {
                        color: Some(palette.background.weak.text),
                    })
                    .into()
            };
            let row_content: Element<'a> = if s.status == StageStatus::Pending {
                column![
                    row![
                        text(icon).size(12),
                        text(&s.label).size(12).style(move |_t: &Theme| {
                            iced_widget::text::Style {
                                color: Some(palette.background.weak.text),
                            }
                        }),
                        text("waiting").size(10).style(move |_t: &Theme| {
                            iced_widget::text::Style {
                                color: Some(palette.background.weak.text),
                            }
                        }),
                    ]
                    .spacing(6),
                    detail,
                ]
                .spacing(4)
                .into()
            } else {
                column![
                    row![
                        text(icon).size(12),
                        progress_bar(0.0..=1.0, s.progress),
                        text(&s.label).size(12),
                    ]
                    .spacing(6),
                    detail,
                ]
                .spacing(4)
                .into()
            };
            row_content
        })
        .collect();

    let content = column(stage_rows).spacing(8).padding(12);

    container(content)
        .width(Length::Fixed(380.0))
        .height(Length::Fixed(160.0))
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
