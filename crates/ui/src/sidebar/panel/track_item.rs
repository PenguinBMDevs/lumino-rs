use iced_core::{Alignment, Length, Padding};
use iced_widget::{button, container, row, space, text};

use crate::{
    Element, Theme,
    resources::icon::{self, Icon},
    sidebar::{Event, Track},
    window,
};

pub fn view<'a>(track: &'a Track, is_selected: bool, window: &window::Window) -> Element<'a> {
    let left_icon: crate::Element<'a> = if track.is_conductor {
        container(icon::view_with_size_and_theme(
            Icon::Clock,
            18,
            18,
            Some(&window.theme),
        ))
        .width(24)
        .align_x(iced_core::alignment::Horizontal::Left)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 2.0,
        })
        .into()
    } else {
        container(
            text("A01")
                .size(14)
                .font(iced_core::Font {
                    weight: iced_core::font::Weight::Bold,
                    ..Default::default()
                })
                .style(move |theme: &Theme| {
                    let palette = theme.extended_palette();
                    let color = if is_selected {
                        palette.background.base.color
                    } else {
                        palette.background.strong.color
                    };
                    text::Style { color: Some(color) }
                }),
        )
        .width(24)
        .align_x(iced_core::alignment::Horizontal::Left)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 2.0,
        })
        .into()
    };

    let name = text(&track.name).size(14).width(Length::Fill);

    let mute_btn = button(
        text("M")
            .size(14)
            .font(iced_core::Font {
                weight: iced_core::font::Weight::Bold,
                ..Default::default()
            })
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                text::Style {
                    color: Some(if track.is_muted {
                        palette.danger.base.color
                    } else {
                        palette.background.strong.color
                    }),
                }
            }),
    )
    .on_press(Event::track_mute_toggled(track.id))
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(0);

    let eye_icon = if track.is_onion_skin_on {
        Icon::Eye
    } else {
        Icon::EyeSlash
    };

    let eye_btn = button(icon::view_with_size_and_theme(
        eye_icon,
        16,
        16,
        Some(&window.theme),
    ))
    .on_press(Event::track_onion_skin_toggled(track.id))
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(0);

    let track_row = row![
        left_icon,
        space().width(4),
        name,
        mute_btn,
        space().width(4),
        eye_btn,
    ]
    .align_y(Alignment::Start)
    .padding(4);

    let track_container = button(track_row)
        .width(Length::Fill)
        .on_press(Event::track_selected(track.id))
        .style(move |theme: &Theme, status| {
            let palette = theme.extended_palette();
            let bg = if is_selected {
                palette.background.strong.color
            } else if status == iced_widget::button::Status::Hovered {
                palette.background.weak.color
            } else {
                palette.background.base.color
            };

            button::Style {
                text_color: palette.background.base.text,
                border: iced_core::Border {
                    radius: 6.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        });

    track_container.into()
}
