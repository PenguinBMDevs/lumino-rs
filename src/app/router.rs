#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Editor,
    Preview,
    Logs,
    Audio,
    Info,
}

#[derive(Debug, Clone)]
pub struct RouteConfig {
    pub route: Route,
    pub icon: &'static str,
}

pub const ROUTES: &[RouteConfig] = &[
    RouteConfig {
        route: Route::Editor,
        icon: "pen-to-square"
    },
    RouteConfig {
        route: Route::Preview,
        icon: "chart-bar"
    },
    RouteConfig {
        route: Route::Logs,
        icon: "file-lines"
    },
    RouteConfig {
        route: Route::Audio,
        icon: "volume-high"
    },
    RouteConfig {
        route: Route::Info,
        icon: "circle-info"
    }
];
