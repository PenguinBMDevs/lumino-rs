/// 侧边栏模块 — 路由、面板、音轨列表
mod core;
mod handling;
mod view;

pub mod event;
mod panel;
mod route;

pub use core::{
    RESIZE_HANDLE_WIDTH,
    ROUTES, Route, RouteConfig, Sidebar, Track,
};
pub use event::Event;

#[cfg(test)]
mod tests {
    use super::*;

    /// 音轨总览模式下：选中音轨不应强制打开侧边栏面板
    #[test]
    fn test_arrangement_mode_set_selected_track_does_not_open_panel() {
        let mut sidebar = Sidebar::new();
        sidebar.route = Route::Arrangement;
        sidebar.panel_visible = false;

        sidebar.set_selected_track(1);

        assert_eq!(sidebar.selected_track, 1);
        assert!(
            !sidebar.panel_visible,
            "Arrangement 模式下 set_selected_track 不应打开面板"
        );
    }

    /// 非音轨总览模式下：选中音轨应打开侧边栏面板
    #[test]
    fn test_non_arrangement_mode_set_selected_track_opens_panel() {
        let mut sidebar = Sidebar::new();
        sidebar.route = Route::File;
        sidebar.panel_visible = false;

        sidebar.set_selected_track(1);

        assert_eq!(sidebar.selected_track, 1);
        assert!(
            sidebar.panel_visible,
            "非 Arrangement 模式下 set_selected_track 应打开面板"
        );
    }

    /// 音轨总览模式下：PanelToggled 事件不应打开面板
    #[test]
    fn test_arrangement_mode_panel_toggled_keeps_panel_closed() {
        let mut sidebar = Sidebar::new();
        sidebar.route = Route::Arrangement;
        sidebar.panel_visible = false;

        sidebar.update(Event::PanelToggled(Route::Arrangement));

        assert!(
            !sidebar.panel_visible,
            "Arrangement 模式下 PanelToggled 不应打开面板"
        );
    }

    /// 音轨总览模式下：RouteUpdated 事件不应打开面板
    #[test]
    fn test_arrangement_mode_route_updated_keeps_panel_closed() {
        let mut sidebar = Sidebar::new();
        sidebar.route = Route::File;
        sidebar.panel_visible = true;

        sidebar.update(Event::RouteUpdated(Route::Arrangement));

        assert!(
            !sidebar.panel_visible,
            "切换到 Arrangement 路由时应关闭面板"
        );
    }

    /// 音轨总览模式下：TrackSelected 事件不应打开面板
    #[test]
    fn test_arrangement_mode_track_selected_keeps_panel_closed() {
        let mut sidebar = Sidebar::new();
        sidebar.route = Route::Arrangement;
        sidebar.panel_visible = false;

        sidebar.update(Event::TrackSelected(1));

        assert_eq!(sidebar.selected_track, 1);
        assert!(
            !sidebar.panel_visible,
            "Arrangement 模式下 TrackSelected 不应打开面板"
        );
    }
}
