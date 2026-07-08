/// 侧边栏模块 — 路由、面板、音轨列表
mod core;
mod handling;
mod view;

pub mod event;
mod panel;
mod route;

pub use core::{GroupId, RESIZE_HANDLE_WIDTH, ROUTES, Route, RouteConfig, Sidebar, Track};
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

    /// 工程走带按钮与音轨列表、自动化面板互斥：打开走带时关闭后两者
    #[test]
    fn test_arrangement_button_closes_file_and_automation_panels() {
        let mut sidebar = Sidebar::new();
        // 初始钢琴卷帘状态：音轨列表面板与自动化面板均打开
        sidebar.panel_visible = true;
        sidebar.panel_route = Route::File;
        sidebar.automation_panel_visible = true;
        sidebar.piano_roll_visible = true;

        sidebar.update(Event::PanelToggled(Route::Arrangement));

        assert_eq!(sidebar.route, Route::Arrangement);
        assert!(!sidebar.panel_visible, "进入工程走带后音轨列表面板应关闭");
        assert!(
            !sidebar.automation_panel_visible,
            "进入工程走带后自动化面板应关闭"
        );
        assert!(!sidebar.piano_roll_visible, "进入工程走带后钢琴卷帘应隐藏");
    }

    /// 再次点击工程走带按钮可恢复之前的钢琴卷帘子按钮状态
    #[test]
    fn test_arrangement_button_restores_file_and_automation_state() {
        let mut sidebar = Sidebar::new();
        sidebar.panel_visible = true;
        sidebar.panel_route = Route::File;
        sidebar.automation_panel_visible = true;
        sidebar.piano_roll_visible = true;

        sidebar.update(Event::PanelToggled(Route::Arrangement));
        sidebar.update(Event::PanelToggled(Route::Arrangement));

        assert_eq!(sidebar.route, Route::File);
        assert!(sidebar.panel_visible, "应恢复音轨列表面板打开状态");
        assert_eq!(sidebar.panel_route, Route::File);
        assert!(sidebar.automation_panel_visible, "应恢复自动化面板打开状态");
        assert!(sidebar.piano_roll_visible, "应恢复钢琴卷帘显示");
    }

    /// 在工程走带界面点击音轨列表按钮：退出走带并恢复钢琴卷帘状态，同时打开音轨列表
    #[test]
    fn test_file_button_exits_arrangement_and_restores_state() {
        let mut sidebar = Sidebar::new();
        sidebar.panel_visible = true;
        sidebar.panel_route = Route::File;
        sidebar.automation_panel_visible = true;

        sidebar.update(Event::PanelToggled(Route::Arrangement));
        sidebar.update(Event::PanelToggled(Route::File));

        assert_eq!(sidebar.route, Route::File);
        assert!(sidebar.panel_visible, "音轨列表面板应被打开");
        assert_eq!(sidebar.panel_route, Route::File);
        assert!(sidebar.automation_panel_visible, "自动化面板状态应被保留");
    }

    /// 在工程走带界面点击自动化面板按钮：退出走带并开启自动化面板
    #[test]
    fn test_automation_button_exits_arrangement_and_turns_on_automation() {
        let mut sidebar = Sidebar::new();
        sidebar.panel_visible = true;
        sidebar.panel_route = Route::File;
        sidebar.automation_panel_visible = false;

        sidebar.update(Event::PanelToggled(Route::Arrangement));
        sidebar.update(Event::AutomationPanelToggled);

        assert_ne!(sidebar.route, Route::Arrangement, "应退出工程走带");
        assert!(sidebar.automation_panel_visible, "自动化面板应被打开");
        assert!(sidebar.piano_roll_visible, "应恢复钢琴卷帘显示");
    }

    /// 从工程走带界面切换到其他分组再切回，应保留钢琴卷帘子按钮状态
    #[test]
    fn test_group_switch_from_arrangement_preserves_piano_roll_state() {
        let mut sidebar = Sidebar::new();
        sidebar.panel_visible = true;
        sidebar.panel_route = Route::File;
        sidebar.automation_panel_visible = true;

        sidebar.update(Event::PanelToggled(Route::Arrangement));
        sidebar.update(Event::GroupToggled(GroupId::Waterfall));
        sidebar.update(Event::GroupToggled(GroupId::PianoRoll));

        assert_eq!(sidebar.route, Route::File);
        assert!(sidebar.panel_visible, "切回钢琴卷帘组后音轨列表面板应恢复");
        assert!(
            sidebar.automation_panel_visible,
            "切回钢琴卷帘组后自动化面板状态应恢复"
        );
    }
}
