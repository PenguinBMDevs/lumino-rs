/// 侧边栏模块 — 路由、面板、音轨列表
mod color_picker;
mod context_menu;
mod core;
pub(crate) mod event_browser;
mod handling;
mod panel;
mod panel_context_menu;
mod view;

pub mod event;
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

    /// 工程走带组按钮与音轨列表、自动化面板互斥：打开走带时关闭后两者
    #[test]
    fn test_arrangement_button_closes_file_and_automation_panels() {
        let mut sidebar = Sidebar::new();
        // 初始钢琴卷帘状态：音轨列表面板与自动化面板均打开
        sidebar.panel_visible = true;
        sidebar.panel_route = Route::File;
        sidebar.automation_panel_visible = true;
        sidebar.piano_roll_visible = true;

        // 工程走带现在通过工程组按钮进入
        sidebar.update(Event::GroupToggled(GroupId::Project));

        assert_eq!(sidebar.route, Route::Arrangement);
        assert!(!sidebar.panel_visible, "进入工程走带后音轨列表面板应关闭");
        assert!(
            !sidebar.automation_panel_visible,
            "进入工程走带后自动化面板应关闭"
        );
        assert!(!sidebar.piano_roll_visible, "进入工程走带后钢琴卷帘应隐藏");
    }

    /// 再次点击工程组按钮可恢复之前的钢琴卷帘子按钮状态
    #[test]
    fn test_arrangement_button_restores_file_and_automation_state() {
        let mut sidebar = Sidebar::new();
        sidebar.panel_visible = true;
        sidebar.panel_route = Route::File;
        sidebar.automation_panel_visible = true;
        sidebar.piano_roll_visible = true;

        // 先后两次点击工程组按钮 = 进入再退出工程走带
        sidebar.update(Event::GroupToggled(GroupId::Project));
        sidebar.update(Event::GroupToggled(GroupId::Project));

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

        sidebar.update(Event::GroupToggled(GroupId::Project));
        // 在工程组中点击音轨列表按钮：跨组点击，应切回钢琴卷帘组并打开音轨列表
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

        sidebar.update(Event::GroupToggled(GroupId::Project));
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

        sidebar.update(Event::GroupToggled(GroupId::Project));
        sidebar.update(Event::GroupToggled(GroupId::Waterfall));
        sidebar.update(Event::GroupToggled(GroupId::PianoRoll));

        assert_eq!(sidebar.route, Route::File);
        assert!(sidebar.panel_visible, "切回钢琴卷帘组后音轨列表面板应恢复");
        assert!(
            sidebar.automation_panel_visible,
            "切回钢琴卷帘组后自动化面板状态应恢复"
        );
    }

    /// 右键菜单打开时记录目标音轨并清除内联编辑状态
    #[test]
    fn test_track_context_menu_opened_sets_target() {
        let mut sidebar = Sidebar::new();
        sidebar.renaming_track = Some((1, "Old".to_string()));
        sidebar.color_picking_track = Some(1);

        sidebar.update(Event::TrackContextMenuOpened(1));

        assert_eq!(sidebar.track_context_menu.target_track_id, Some(1));
        assert!(sidebar.renaming_track.is_none());
        assert!(sidebar.color_picking_track.is_none());
    }

    /// 关闭右键菜单后目标音轨被清空
    #[test]
    fn test_track_context_menu_closed_clears_target() {
        let mut sidebar = Sidebar::new();
        sidebar.update(Event::TrackContextMenuOpened(1));
        sidebar.update(Event::TrackContextMenuClosed);

        assert!(sidebar.track_context_menu.target_track_id.is_none());
    }

    /// 删除菜单项会移除可删除音轨并保持选中音轨有效
    #[test]
    fn test_track_context_menu_delete_removes_track() {
        use lumino_message::TrackContextMenuItem;

        let mut sidebar = Sidebar::new();
        sidebar.selected_track = 1;

        sidebar.update(Event::TrackContextMenuItemClicked(
            1,
            TrackContextMenuItem::Delete,
        ));

        assert!(!sidebar.tracks.iter().any(|t| t.id == 1));
        assert_eq!(sidebar.selected_track, sidebar.tracks[0].id);
    }

    /// 不可删除的音轨不会被删除
    #[test]
    fn test_track_context_menu_delete_respects_can_delete() {
        use lumino_message::TrackContextMenuItem;

        let mut sidebar = Sidebar::new();
        let conductor_id = sidebar.tracks[0].id;
        assert!(!sidebar.tracks[0].can_delete);

        sidebar.update(Event::TrackContextMenuItemClicked(
            conductor_id,
            TrackContextMenuItem::Delete,
        ));

        assert!(sidebar.tracks.iter().any(|t| t.id == conductor_id));
    }

    /// 重命名流程会更新音轨名称
    #[test]
    fn test_track_rename_flow_updates_name() {
        let mut sidebar = Sidebar::new();
        let track_id = sidebar.tracks[1].id;

        sidebar.update(Event::TrackRenameStarted(track_id));
        assert_eq!(sidebar.renaming_track.as_ref().unwrap().1, "Setup");

        sidebar.update(Event::TrackRenameChanged(track_id, "New Name".to_string()));
        assert_eq!(sidebar.renaming_track.as_ref().unwrap().1, "New Name");

        sidebar.update(Event::TrackRenameConfirmed(track_id));
        assert_eq!(
            sidebar
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .unwrap()
                .name,
            "New Name"
        );
        assert!(sidebar.renaming_track.is_none());
    }

    /// 取消重命名不会修改音轨名称
    #[test]
    fn test_track_rename_cancelled_leaves_name() {
        let mut sidebar = Sidebar::new();
        let track_id = sidebar.tracks[1].id;
        let original_name = sidebar.tracks[1].name.clone();

        sidebar.update(Event::TrackRenameStarted(track_id));
        sidebar.update(Event::TrackRenameChanged(track_id, "New Name".to_string()));
        sidebar.update(Event::TrackRenameCancelled(track_id));

        assert_eq!(
            sidebar
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .unwrap()
                .name,
            original_name
        );
        assert!(sidebar.renaming_track.is_none());
    }

    /// 颜色选择会设置音轨选项卡颜色
    #[test]
    fn test_track_color_selected_sets_color() {
        use iced_core::Color;

        let mut sidebar = Sidebar::new();
        let track_id = sidebar.tracks[1].id;
        let color = Color::from_rgb(0.5, 0.5, 0.5);

        sidebar.update(Event::TrackColorPickerOpened(track_id));
        assert_eq!(sidebar.color_picking_track, Some(track_id));

        sidebar.update(Event::TrackColorSelected(track_id, color));
        assert_eq!(
            sidebar
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .unwrap()
                .color,
            Some(color)
        );
        assert!(sidebar.color_picking_track.is_none());
    }

    /// 重置颜色会清除音轨选项卡颜色并关闭选择器
    #[test]
    fn test_track_color_reset_clears_color() {
        use iced_core::Color;

        let mut sidebar = Sidebar::new();
        let track_id = sidebar.tracks[1].id;

        sidebar.update(Event::TrackColorPickerOpened(track_id));
        sidebar.update(Event::TrackColorSelected(
            track_id,
            Color::from_rgb(0.5, 0.5, 0.5),
        ));
        assert!(
            sidebar
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .unwrap()
                .color
                .is_some()
        );

        sidebar.update(Event::TrackColorPickerOpened(track_id));
        sidebar.update(Event::TrackColorReset(track_id));
        assert!(
            sidebar
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .unwrap()
                .color
                .is_none()
        );
        assert!(sidebar.color_picking_track.is_none());
    }
}
