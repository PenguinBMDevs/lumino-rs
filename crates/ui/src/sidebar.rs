use iced_core::{Border, Color, Length};
use iced_widget::{button, column, container, row, space};

use crate::{
    Element,
    Theme,
    Message,
    resources::icon
};

#[derive(Debug, Clone)]
pub enum Event {
    RouteUpdated(Route)
}

impl Event {
    pub const fn route_updated(r: Route) -> Message {
        Message::Sidebar(Self::RouteUpdated(r))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    File,
    Audio
}

#[derive(Debug, Clone)]
pub struct RouteConfig {
    pub route: Route,
    pub icon: icon::Icon,
}

const ROUTES: [RouteConfig; 2] = [
    RouteConfig {
        route: Route::File,
        icon: icon::FolderTree,
    },
    RouteConfig {
        route: Route::Audio,
        icon: icon::WaveForm,
    }
];

pub struct Sidebar {
    route: Route
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            route: Route::File,
        }
    }

    pub fn view(&self) -> Element<'_> {
        let items = ROUTES
            .iter()
            .map(|r| item(r, r.route == self.route))
            .collect::<Vec<_>>();

        let inner = column(items).spacing(4);

        container(inner)
            .width(46)
            .height(Length::Fill)
            .into()
    }

    pub fn update(&mut self, event: Event) {
        use Event::*;
        match event {
            RouteUpdated(r) => self.route = r,
        }
    }
}

/*
    button: 46x40
    split: 3x16
    icon: 16x16
    padding-y: 12
    padding-left: 3+12
    padding-right: 12
*/

fn item<'a>(cfg: &'a RouteConfig, active: bool) -> Element<'a> {
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

    let inner = row![split, icon(cfg.icon)].spacing(12);

    button(inner)
        .width(46)
        .height(40)
        .padding([12, 0])
        .style(move |theme: &Theme, status| {
            use button::Status::*;

            let palette = theme.extended_palette();
            let background = match (active, status) {
                (true, Hovered) | (false, Pressed) => palette.background.weak.color,

                (true, _) | (false, Hovered) => palette.background.weaker.color,

                _ => Color::TRANSPARENT,
            };

            button::Style {
                border: Border::default().rounded(6),
                ..Default::default()
            }
            .with_background(background)
        })
        .on_press(Event::route_updated(cfg.route))
        .into()
}
