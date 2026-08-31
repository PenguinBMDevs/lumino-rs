//! 加载错误对话框 — yinhe `dialogs/load_error.rs:73` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染加载错误对话框
pub fn view<'a>(window: &'a Window, error: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        text(error).size(13),
        iced_widget::Space::new().height(12),
        row![
            iced_widget::Space::new().width(Length::Fill),
            button(text("ok").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 14])
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
        ],
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fixed(396.0));

    container(content)
        .width(Length::Fixed(420.0))
        .height(Length::Fixed(120.0))
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
