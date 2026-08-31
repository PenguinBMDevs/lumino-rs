//! 渲染设置页 — yinhe `dialogs/settings/render.rs:61` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{checkbox, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        text("render").size(14),
        row![
            text("vsync").size(12).width(Length::Fixed(140.0)),
            checkbox(true)
                .label("enable")
                .on_toggle(|_| lumino_ui_core::message::null()),
        ]
        .spacing(8),
        row![
            text("gpu_accelerated").size(12).width(Length::Fixed(140.0)),
            checkbox(true)
                .label("enable")
                .on_toggle(|_| lumino_ui_core::message::null()),
        ]
        .spacing(8),
        text("note: requires restart")
            .size(10)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }),
    ]
    .spacing(10)
    .padding(12);

    container(content)
        .width(Length::Fill)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}
