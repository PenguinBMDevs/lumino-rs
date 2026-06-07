use iced_core::{Alignment, Length, Padding};
use iced_widget::{button, container, row, space, text};

use crate::{
    widget, Element, Theme,
    resources::icon::{self, Icon},
    sidebar::Event,
    window,
};

pub fn view<'a>(is_expanded: bool, window: &window::Window) -> Element<'a> {
    let plus_icon = container(icon::view_with_size_and_theme(
        Icon::Plus,
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
    });

    let add_track_row = if is_expanded {
        row![
            plus_icon,
            space().width(4),
            text("添加音轨").size(14).width(Length::Fill),
            container(
                widget::with_tooltip_bottom(
                    button(icon::view_with_size_and_theme(
                        Icon::EllipsisVertical,
                        15,
                        15,
                        Some(&window.theme),
                    ))
                    .on_press(Event::add_track())
                    .style(|_theme: &Theme, _status| {
                        button::Style::default()
                            .with_background(iced_core::Color::from_rgb(0.84, 0.84, 0.84))
                    })
                    .padding(2),
                    "音轨选项",
                ),
            )
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default()
                    .background(palette.background.weak.color)
                    .border(iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    })
            })
            .padding(2),
        ]
        .align_y(Alignment::Center)
        .padding(6)
    } else {
        row![
            plus_icon,
            space().width(4),
            text("添加音轨").size(14).width(Length::Fill),
            container(
                widget::with_tooltip_bottom(
                    button(icon::view_with_size_and_theme(
                        Icon::EllipsisVertical,
                        18,
                        18,
                        Some(&window.theme),
                    ))
                    .on_press(Event::add_track_menu_toggled())
                    .style(|_theme: &Theme, _status| {
                        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
                    })
                    .padding(0),
                    "音轨选项",
                ),
            ),
        ]
        .align_y(Alignment::Center)
        .padding(6)
    };

    let on_press = if is_expanded {
        Event::add_track_menu_toggled()
    } else {
        Event::add_track()
    };

    let add_track_container = button(add_track_row)
        .width(Length::Fill)
        .on_press(on_press)
        .style(|theme: &Theme, status| {
            let palette = theme.extended_palette();
            let bg = if status == iced_widget::button::Status::Hovered {
                palette.background.weak.color
            } else {
                palette.background.base.color
            };

            button::Style {
                text_color: palette.background.base.text,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        });

    add_track_container.into()
}
