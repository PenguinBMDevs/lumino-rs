//! 音频设备切换对话框 — yinhe `dialogs/audio_device_switch.rs:177` 的 iced 迁移桩

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, scrollable, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染音频设备切换对话框
pub fn view<'a>(
    window: &'a Window,
    devices: &'a [String],
    error: Option<&'a str>,
    allow_keep_current: bool,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let intro: Element<'a> = if allow_keep_current {
        column![
            text("devices_changed").size(13),
            text("select_new").size(12).style(move |_t: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }),
        ]
        .spacing(4)
        .into()
    } else {
        column![
            text("stream_error").size(13),
            text("select_device").size(12).style(move |_t: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }),
        ]
        .spacing(4)
        .into()
    };

    let device_list: Element<'a> =
        if devices.is_empty() {
            container(text("no_devices").size(12).style(move |_t: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(12)
            .into()
        } else {
            let rows: Vec<Element<'a>> = devices
                .iter()
                .map(|name| {
                    button(text(name).size(12))
                        .on_press(lumino_ui_core::message::null())
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(move |_t: &Theme, status| {
                            let c = match status {
                                button::Status::Hovered => weak,
                                _ => iced_core::Color::TRANSPARENT,
                            };
                            button::Style {
                                background: Some(iced_core::Background::Color(c)),
                                border: iced_core::Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        })
                        .into()
                })
                .collect();
            container(scrollable(column(rows).spacing(4)).height(Length::Fixed(200.0)))
                .width(Length::Fill)
                .style(move |_t: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(weak.scale_alpha(0.35))),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

    let error_row: Element<'a> = if let Some(err) = error {
        text(err)
            .size(11)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.danger.base.color),
            })
            .into()
    } else {
        iced_widget::Space::new().height(0).into()
    };

    let keep_btn: Element<'a> = if allow_keep_current {
        button(text("keep_current").size(12))
            .on_press(lumino_ui_core::message::null())
            .padding([6, 12])
            .into()
    } else {
        iced_widget::Space::new().width(Length::Fixed(0.0)).into()
    };
    let content = column![
        intro,
        iced_widget::Space::new().height(6),
        device_list,
        error_row,
        iced_widget::Space::new().height(8),
        row![
            button(text("exit_app").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12]),
            iced_widget::Space::new().width(Length::Fill),
            keep_btn,
            button(text("refresh_devices").size(12))
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
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .padding(12);

    container(content)
        .width(Length::Fixed(460.0))
        .height(Length::Fixed(440.0))
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
