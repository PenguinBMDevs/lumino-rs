use iced_widget::{container, row};

mod panel;
mod route;

use crate::{Element, Message, resources::icon, window};

#[derive(Debug, Clone)]
pub enum Event {
    RouteUpdated(Route),
    PanelToggled(Route),
    TrackSelected(usize),
    TrackMuteToggled(usize),
    TrackOnionSkinToggled(usize),
    AddTrack,
    AddTrackMenuToggled,
}

impl Event {
    pub const fn route_updated(r: Route) -> Message {
        Message::Sidebar(Self::RouteUpdated(r))
    }

    pub const fn panel_toggled(r: Route) -> Message {
        Message::Sidebar(Self::PanelToggled(r))
    }

    pub const fn track_selected(id: usize) -> Message {
        Message::Sidebar(Self::TrackSelected(id))
    }

    pub const fn track_mute_toggled(id: usize) -> Message {
        Message::Sidebar(Self::TrackMuteToggled(id))
    }

    pub const fn track_onion_skin_toggled(id: usize) -> Message {
        Message::Sidebar(Self::TrackOnionSkinToggled(id))
    }

    pub const fn add_track() -> Message {
        Message::Sidebar(Self::AddTrack)
    }

    pub const fn add_track_menu_toggled() -> Message {
        Message::Sidebar(Self::AddTrackMenuToggled)
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
    pub route: Route,
    panel_visible: bool,
    panel_route: Route,
    pub tracks: Vec<Track>,
    pub selected_track: usize,
    pub add_track_menu_open: bool,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub is_conductor: bool,
    pub is_muted: bool,
    pub is_onion_skin_on: bool,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            route: Route::File,
            panel_visible: true,
            panel_route: Route::File,
            tracks: vec![
                Track {
                    id: 0,
                    name: "Conductor".to_string(),
                    is_conductor: true,
                    is_muted: false,
                    is_onion_skin_on: true,
                },
                Track {
                    id: 1,
                    name: "Setup".to_string(),
                    is_conductor: false,
                    is_muted: false,
                    is_onion_skin_on: true,
                },
            ],
            selected_track: 0,
            add_track_menu_open: false,
        }
    }

    pub fn view(&self, window: &window::Window) -> Element<'_> {
        let panel = if self.panel_visible {
            panel::view(
                self.panel_route,
                &self.tracks,
                self.selected_track,
                self.add_track_menu_open,
                window,
            )
        } else {
            iced_widget::container(iced_widget::space()).width(0).into()
        };

        let inner = row![route::view(self.route, window), panel,];

        container(inner).into()
    }

    pub fn width(&self) -> u32 {
        48 + if self.panel_visible { 200 } else { 0 }
    }

    pub fn update(&mut self, event: Event) -> bool {
        use Event::*;
        let prev_visible = self.panel_visible;
        match event {
            RouteUpdated(r) => self.route = r,
            PanelToggled(r) => {
                if self.panel_visible && self.panel_route == r {
                    self.panel_visible = false;
                } else {
                    self.panel_visible = true;
                    self.panel_route = r;
                    self.route = r;
                }
            }
            TrackSelected(id) => {
                tracing::debug!("Sidebar: TrackSelected id={}", id);
                self.selected_track = id;
            }
            TrackMuteToggled(id) => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.is_muted = !track.is_muted;
                }
            }
            TrackOnionSkinToggled(id) => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.is_onion_skin_on = !track.is_onion_skin_on;
                }
            }
            AddTrack => {
                // 添加新音轨
                let new_id = self.tracks.len();
                self.tracks.push(Track {
                    id: new_id,
                    name: format!("Track {}", new_id),
                    is_conductor: false,
                    is_muted: false,
                    is_onion_skin_on: true,
                });
                self.selected_track = new_id;
                self.add_track_menu_open = false;
            }
            AddTrackMenuToggled => {
                self.add_track_menu_open = !self.add_track_menu_open;
            }
        }
        self.panel_visible != prev_visible
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Sidebar {
    /// 检查当前是否为设置路由且面板可见
    pub fn is_settings_route(&self) -> bool {
        self.route == Route::Settings && self.panel_visible
    }

    /// 从 MIDI 数据更新音轨列表
    pub fn update_tracks_from_midi(&mut self, track_infos: &[(usize, Option<String>, u64)]) {
        tracing::info!("update_tracks_from_midi: {} tracks", track_infos.len());
        self.tracks.clear();
        for (idx, (track_idx, name, _note_count)) in track_infos.iter().enumerate() {
            let track_name = name.as_deref().unwrap_or("Unknown");
            tracing::debug!("  track {}: id={}, name={}", idx, track_idx, track_name);
            self.tracks.push(Track {
                id: *track_idx,
                name: format!("{:02} {}", idx + 1, track_name),
                is_conductor: *track_idx == 0, // 第一个音轨作为 conductor
                is_muted: false,
                is_onion_skin_on: true,
            });
        }
        // 如果有音轨，默认选择第一个
        if !self.tracks.is_empty() {
            self.selected_track = self.tracks[0].id;
            tracing::info!("default selected_track = {}", self.selected_track);
        }
    }

    /// 设置当前选中的音轨
    pub fn set_selected_track(&mut self, track_idx: usize) {
        self.selected_track = track_idx;
        // 确保选中的音轨在面板中可见
        self.panel_visible = true;
    }

    /// 获取所有音轨的洋葱皮开关状态
    ///
    /// 返回一个 HashMap，key 是音轨 ID，value 是洋葱皮是否启用
    pub fn get_onion_skin_states(&self) -> std::collections::HashMap<usize, bool> {
        let mut states = std::collections::HashMap::new();
        for track in &self.tracks {
            states.insert(track.id, track.is_onion_skin_on);
        }
        states
    }
}
