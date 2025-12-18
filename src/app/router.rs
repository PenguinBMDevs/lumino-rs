use crate::resources::icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Editor,
    Preview,
    Logs,
    Audio,
}

#[derive(Debug, Clone)]
pub struct RouteConfig {
    pub route: Route,
    pub icon: icon::Icon,
}

pub const ROUTES: &[RouteConfig] = &[
    RouteConfig {
        route: Route::Editor,
        icon: icon::PenToSquare,
    },
    RouteConfig {
        route: Route::Preview,
        icon: icon::ChartBar,
    },
    RouteConfig {
        route: Route::Logs,
        icon: icon::FileLines,
    },
    RouteConfig {
        route: Route::Audio,
        icon: icon::MusicNote,
    },
];
