use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, row, space};
use lumino_core::i18n::Language;

use super::{Event, ROUTES, Route, RouteConfig};

use crate::widget;
use crate::{Element, Theme, resources::icon, window};

pub fn view<'a>(
    active: Route,
    panel_visible: bool,
    window: &window::Window,
    language: Language,
) -> Element<'a> {
    let items = ROUTES
        .into_iter()
        .map(|r| match r {
            RouteConfig::Item { route, icon } => item(
                route,
                icon,
                panel_visible && route == active,
                window,
                language,
            ),
            RouteConfig::Space => space().height(Length::Fill).into(),
        })
        .collect::<Vec<_>>();

    container(column(items))
        .width(48)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weaker.color)
        })
        .into()
}

fn item<'a>(
    route: Route,
    icon_enum: icon::Icon,
    active: bool,
    window: &window::Window,
    language: Language,
) -> Element<'a> {
    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let background = match active {
                true => palette.primary.base.color,
                false => Color::TRANSPARENT,
            };

            container::Style::default().background(background)
        });

    let icon_img = icon::view_with_size_and_theme(icon_enum, 20, 20, Some(&window.theme));

    let inner = row![split, icon_img,]
        .spacing(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    // 音轨总览路由使用 RouteUpdated 事件，避免触发面板切换
    let event = if route == Route::Arrangement {
        Event::route_updated(route)
    } else {
        Event::panel_toggled(route)
    };

    let btn = button(inner)
        .width(48)
        .height(48)
        .padding(0)
        .style(move |theme: &Theme, status| {
            use button::Status::*;
            let palette = theme.extended_palette();
            let text_color = match status {
                Hovered | Pressed => palette.background.base.color,
                _ => palette.background.weakest.color,
            };
            button::Style {
                text_color,
                ..Default::default()
            }
            .with_background(Color::TRANSPARENT)
        })
        .on_press(event);

    widget::with_tooltip(
        btn,
        route.tooltip(language),
        iced_widget::tooltip::Position::Right,
    )
    .into()
}
