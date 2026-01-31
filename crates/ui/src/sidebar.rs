use iced_widget::{container, row};

mod panel;
mod route;

use crate::{Element, Message, resources::icon};

#[derive(Debug, Clone)]
pub enum Event {
    RouteUpdated(Route),
    PanelToggled(Route),
}

impl Event {
    pub const fn route_updated(r: Route) -> Message {
        Message::Sidebar(Self::RouteUpdated(r))
    }

    pub const fn panel_toggled(r: Route) -> Message {
        Message::Sidebar(Self::PanelToggled(r))
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
    panel_visible: bool,
    panel_route: Route,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            route: Route::File,
            panel_visible: true,
            panel_route: Route::File,
        }
    }

    pub fn view(&self) -> Element<'_> {
        let panel = if self.panel_visible {
            panel::view()
        } else {
            iced_widget::container(iced_widget::space())
                .width(0)
                .into()
        };

        let inner = row![route::view(self.route), panel,];

        container(inner).into()
    }

    pub fn update(&mut self, event: Event) {
        use Event::*;
        match event {
            RouteUpdated(r) => self.route = r,
            PanelToggled(r) => {
                if r == Route::Settings {
                    self.route = r;
                } else {
                    if self.panel_visible && self.panel_route == r {
                        self.panel_visible = false;
                    } else {
                        self.panel_visible = true;
                        self.panel_route = r;
                        self.route = r;
                    }
                }
            }
        }
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
