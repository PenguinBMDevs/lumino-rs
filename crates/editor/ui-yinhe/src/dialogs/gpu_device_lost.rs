//! GPU 设备丢失对话框 — yinhe `dialogs/gpu_device_lost.rs:82` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        text("gpu_lost message").size(13),
        text("gpu_lost action")
            .size(12)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }),
        iced_widget::Space::new().height(12),
        row![
            iced_widget::Space::new().width(Length::Fill),
            button(text("exit_app").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 14])
                .style(move |_t: &Theme, status| {
                    let c = match status {
                        button::Status::Hovered => palette.danger.strong.color,
                        _ => palette.danger.base.color,
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
    .padding(16);

    container(content)
        .width(Length::Fixed(460.0))
        .height(Length::Fixed(150.0))
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
