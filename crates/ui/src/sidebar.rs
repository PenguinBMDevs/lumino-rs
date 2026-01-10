use iced_widget::{container, row};

mod panel;
mod route;

use crate::{Element, Message, resources::icon};

#[derive(Debug, Clone)]
pub enum Event {
    RouteUpdated(Route),
}

impl Event {
    pub const fn route_updated(r: Route) -> Message {
        Message::Sidebar(Self::RouteUpdated(r))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    File,
    Audio,
    Settings,
}

#[derive(Debug, Clone)]
pub enum RouteConfig {
    Item { route: Route, icon: icon::Icon },
    Space,
}

const ROUTES: [RouteConfig; 4] = [
    RouteConfig::Item {
        route: Route::File,
        icon: icon::FolderTree,
    },
    RouteConfig::Item {
        route: Route::Audio,
        icon: icon::WaveForm,
    },
    RouteConfig::Space,
    RouteConfig::Item {
        route: Route::Settings,
        icon: icon::Gear,
    },
];

pub struct Sidebar {
    route: Route,
}

impl Sidebar {
    pub fn new() -> Self {
        Self { route: Route::File }
    }

    pub fn view(&self) -> Element<'_> {
        let inner = row![route::view(self.route), panel::view(),];

        container(inner).into()
    }

    pub fn update(&mut self, event: Event) {
        use Event::*;
        match event {
            RouteUpdated(r) => self.route = r,
        }
    }
}
