use iced::{
    Border, Color, Element, Theme, widget::{
        button, column, container, row, space
    }
};

use crate::{app::{
    message::Message,
    router:: {
        ROUTES,
        Route,
        RouteConfig
    }
}, resources::icon};

/* we'll allow the sidebar to expand for showing the tab hints. */
pub fn view<'a>(
    route: &Route
) -> Element<'a, Message> {
    let items = ROUTES.iter()
        .map(|cfg| item(cfg, *route == cfg.route))
        .collect::<Vec<_>>();

    let inner = column(items)
        .spacing(4);

    container(inner)
        .width(46)
        .into()
}

/*
    button: 46x40
    split: 3x16
    icon: 16x16
    padding-y: 12
    padding-left: 3+12
    padding-right: 12
*/

fn item<'a>(
    cfg: &'a RouteConfig,
    active: bool,
) -> Element<'a, Message> {
    let split = container(space())
        .width(3)
        .height(16)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let background = match active {
                true => palette.primary.base.color,
                false => Color::TRANSPARENT,
            };

            container::Style::default()
                .border(Border::default().rounded(1.5))
                .background(background)
        });

    let inner = row![
        split,
        icon(cfg.icon)
    ]
        .spacing(12);

    button(inner)
        .width(46)
        .height(40)
        .padding([12, 0])
        .style(move |theme: &Theme, status| {
            use button::Status::*;

            let palette = theme.extended_palette();
            let background = match (active, status) {
                (true, Hovered) | (false, Pressed) =>
                    palette.background.weak.color,

                (true, _) | (false, Hovered) =>
                    palette.background.weaker.color,

                _ => Color::TRANSPARENT,
            };

            button::Style {
                border: Border::default().rounded(6),
                ..Default::default()
            }
                .with_background(background)
        })
        .on_press(Message::RouteUpdated(cfg.route))
        .into()
}
