use iced_widget::{container, row};
use lumino_extras::i18n::Language;

use super::core::{ROUTE_BAR_WIDTH, Sidebar};
use super::event_browser;
use super::{panel, route};
use crate::titlebar::mode_toggle::AppMode;
use crate::{Element, window};

impl Sidebar {
    /// 返回完整的侧边栏视图（包括路由图标栏和面板）
    #[allow(clippy::too_many_arguments)]
    pub fn view<'a>(
        &'a self,
        window: &'a window::Window,
        language: Language,
        current_mode: AppMode,
        editor_data: &'a lumino_editor_state::EditorData,
        current_track_notes: &'a im::Vector<lumino_note_core::Note>,
        ppq: u16,
        _snap_precision: f32,
        program_font_name: &'a str,
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
                event_browser_state: &self.event_browser_state,
                event_browser_data: event_browser::EventBrowserData {
                    tracks: &self.tracks,
                    current_track_notes,
                    ppq,
                    time_signatures: &editor_data.time_signatures,
                    tempo_points: &editor_data.tempo_points,
                    key_signatures: &editor_data.key_signatures,
                    markers: &editor_data.markers,
                    lyrics: &editor_data.lyrics,
                    chords: &editor_data.chords,
                    program_changes: &editor_data.program_changes,
                    automation_lanes: &editor_data.automation_lanes,
                },
                event_list_context_menu_tick: self.event_list_context_menu_tick,
                program_font_name,
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
