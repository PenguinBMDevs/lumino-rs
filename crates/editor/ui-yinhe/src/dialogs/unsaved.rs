//! 未保存确认对话框 — yinhe `dialogs/unsaved.rs:103` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        text("unsaved changes – save before leaving?")
            .size(13)
            .width(Length::Fixed(316.0)),
        iced_widget::Space::new().height(8),
        row![
            button(text("save").size(12))
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
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }),
            button(text("discard").size(12).style(move |_t: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.danger.base.color),
                }
            }))
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
    .width(Length::Fixed(316.0));

    container(content)
        .width(Length::Fixed(340.0))
        .height(Length::Fixed(130.0))
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
