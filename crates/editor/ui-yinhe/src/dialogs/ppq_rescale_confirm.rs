//! PPQ 缩放确认对话框 — yinhe `dialogs/ppq_rescale_confirm.rs:125` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window, old: u32, new: u32) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        text(format!("ppq_rescale desc: {old} → {new}")).size(13),
        text("ppq_rescale question").size(12),
        text("hint: rescale keeps musical time")
            .size(10)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }),
        iced_widget::Space::new().height(8),
        row![
            button(text("yes").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12])
                .style(move |_t: &Theme, status| {
                    let c = match status {
                        button::Status::Hovered => palette.primary.strong.color,
                        _ => palette.primary.base.color,
                    };
                    button::Style {
                        background: Some(iced_core::Background::Color(c)),
                        text_color: iced_core::Color::WHITE,
                        ..Default::default()
                    }
                }),
            button(text("no").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12]),
            iced_widget::Space::new().width(Length::Fill),
            button(text("cancel").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12]),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fixed(356.0));

    container(content)
        .width(Length::Fixed(380.0))
        .height(Length::Fixed(170.0))
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
