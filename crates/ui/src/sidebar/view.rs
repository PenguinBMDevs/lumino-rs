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
        current_track_notes: &'a lumino_core::im::Vector<lumino_core::Note>,
        ppq: u16,
        snap_precision: f32,
    ) -> Element<'a> {
        let panel = if self.panel_visible {
            let sidebar_params = panel::SidebarViewParams {
                route: self.panel_route,
                tracks: &self.tracks,
                selected_track: self.selected_track,
                panel_width: self.panel_width,
                is_resizing: self.is_resizing,
                context_menu_target_id: self.track_context_menu.target_track_id,
                renaming_track: self.renaming_track.as_ref(),
                color_picking_track: self.color_picking_track,
                current_track_notes,
                ppq,
                snap_precision,
                event_list_scroll_y: self.event_list_scroll_y,
                event_list_viewport_height: self.event_list_viewport_height,
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
                self.pitch_bend_panel_visible,
                self.piano_roll_visible,
                current_mode,
                self.active_group,
                self.audio_export_visible,
                self.video_export_visible,
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
