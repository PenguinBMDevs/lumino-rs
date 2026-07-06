use iced_widget::{container, row};
use lumino_core::i18n::Language;

use super::core::{ROUTE_BAR_WIDTH, Sidebar};
use super::{panel, route};
use crate::titlebar::mode_toggle::AppMode;
use crate::{Element, window};

impl Sidebar {
    /// 返回完整的侧边栏视图（包括路由图标栏和面板）
    pub fn view<'a>(
        &'a self,
        window: &'a window::Window,
        language: Language,
        current_mode: AppMode,
    ) -> Element<'a> {
        let panel = if self.panel_visible {
            let sidebar_params = panel::SidebarViewParams {
                route: self.panel_route,
                tracks: &self.tracks,
                selected_track: self.selected_track,
                add_track_menu_open: self.add_track_menu_open,
                panel_width: self.panel_width,
                is_resizing: self.is_resizing,
            };
            panel::view(sidebar_params, window, language)
        } else {
            iced_widget::container(iced_widget::space()).width(0).into()
        };

        let inner = row![
            route::view(
                self.route,
                self.panel_visible,
                self.automation_panel_visible,
                self.piano_roll_visible,
                current_mode,
                window,
                language
            ),
            panel,
        ];

        container(inner).into()
    }

    pub fn width(&self) -> u32 {
        (ROUTE_BAR_WIDTH
            + if self.panel_visible {
                self.panel_width
            } else {
                0.0
            }) as u32
    }
}
