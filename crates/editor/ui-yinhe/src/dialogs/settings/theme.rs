//! 主题设置页 — yinhe `dialogs/settings/theme.rs:131` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染主题设置页
pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let preset_row = row![
        text("theme_preset").size(12),
        container(text("Dark").size(12))
            .padding(6)
            .style(move |_t: &Theme| container::Style {
                background: Some(iced_core::Background::Color(weak)),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        button(text("custom").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8]),
    ]
    .spacing(8);

    let color_grid = column![
        color_row(window, "bg"),
        color_row(window, "text"),
        color_row(window, "accent"),
        color_row(window, "danger"),
        color_row(window, "warning"),
    ]
    .spacing(6);

    let scale_row = row![
        text("ui_scale").size(12),
        button(text("-").size(12))
            .on_press(lumino_ui_core::message::null())
            .padding(4),
        text("100%").size(12),
        button(text("+").size(12))
            .on_press(lumino_ui_core::message::null())
            .padding(4),
    ]
    .spacing(6);

    let content = column![text("theme").size(14), preset_row, color_grid, scale_row,]
        .spacing(12)
        .padding(12);

    container(content)
        .width(Length::Fill)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}

fn color_row<'a>(window: &'a Window, label: &'static str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    row![
        text(label).size(12).width(Length::Fixed(80.0)),
        container(iced_widget::Space::new().width(28).height(20)).style(move |_t: &Theme| {
            container::Style {
                background: Some(iced_core::Background::Color(palette.primary.base.color)),
                border: iced_core::Border {
                    radius: 3.0.into(),
                    width: 1.0,
                    color: palette.background.weak.color,
                },
                ..Default::default()
            }
        }),
        button(text("edit").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8]),
    ]
    .spacing(8)
    .into()
}
