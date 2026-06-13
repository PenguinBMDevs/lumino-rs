use iced_widget::{container, row};

pub mod event;
mod panel;
mod route;

use crate::{Element, resources::icon, window};
pub use event::Event;

/// 路由栏宽度（固定）
pub const ROUTE_BAR_WIDTH: f32 = 48.0;
/// 面板默认宽度
pub const DEFAULT_PANEL_WIDTH: f32 = 200.0;
/// 面板最小宽度
pub const MIN_PANEL_WIDTH: f32 = 150.0;
/// 面板最大宽度
pub const MAX_PANEL_WIDTH: f32 = 400.0;
/// 调整大小手柄宽度
pub const RESIZE_HANDLE_WIDTH: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    File,
    Arrangement,
    Audio,
}

impl Route {
    pub fn tooltip(&self) -> &'static str {
        match self {
            Route::File => "文件管理",
            Route::Arrangement => "音轨总览",
            Route::Audio => "音频设置",
        }
    }
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
        route: Route::Arrangement,
        icon: icon::Arrangement,
    },
    RouteConfig::Item {
        route: Route::Audio,
        icon: icon::WaveForm,
    },
    RouteConfig::Space,
];

pub struct Sidebar {
    pub route: Route,
    panel_visible: bool,
    panel_route: Route,
    pub tracks: Vec<Track>,
    pub selected_track: usize,
    pub add_track_menu_open: bool,
    /// 面板宽度（默认 200）
    pub panel_width: f32,
    /// 是否正在拖拽调整宽度
    is_resizing: bool,
    /// 拖拽开始时的鼠标 X 坐标
    resize_start_x: f32,
    /// 拖拽开始时的面板宽度
    resize_start_width: f32,
    /// 音轨列表滚动偏移（虚拟滚动）
    track_scroll_offset: f32,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub is_conductor: bool,
    pub can_delete: bool,
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
                    can_delete: false,
                    is_muted: false,
                    is_onion_skin_on: true,
                },
                Track {
                    id: 1,
                    name: "Setup".to_string(),
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                    is_onion_skin_on: true,
                },
            ],
            selected_track: 0,
            add_track_menu_open: false,
            panel_width: DEFAULT_PANEL_WIDTH,
            is_resizing: false,
            resize_start_x: 0.0,
            resize_start_width: DEFAULT_PANEL_WIDTH,
            track_scroll_offset: 0.0,
        }
    }

    /// 返回完整的侧边栏视图（包括路由图标栏和面板）
    pub fn view<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        let panel = if self.panel_visible {
            let sidebar_params = panel::SidebarViewParams {
                route: self.panel_route,
                tracks: &self.tracks,
                selected_track: self.selected_track,
                add_track_menu_open: self.add_track_menu_open,
                panel_width: self.panel_width,
                is_resizing: self.is_resizing,
                scroll_offset: self.track_scroll_offset,
            };
            panel::view(sidebar_params, window)
        } else {
            iced_widget::container(iced_widget::space()).width(0).into()
        };

        let inner = row![route::view(self.route, self.panel_visible, window), panel,];

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

    pub fn update(&mut self, event: Event) -> bool {
        use Event::*;
        let prev_visible = self.panel_visible;
        let prev_route = self.route;
        match event {
            RouteUpdated(r) => {
                self.route = r;
                // 切换到音轨总览路由时，自动隐藏左侧面板
                if r == Route::Arrangement {
                    self.panel_visible = false;
                }
            }
            PanelToggled(r) => {
                // 音轨总览模式下：点击其他路由按钮只切换路由，不打开面板
                if self.route == Route::Arrangement && r != Route::Arrangement {
                    self.route = r;
                } else if r == Route::Arrangement {
                    // 切换到音轨总览路由时，关闭面板
                    self.panel_visible = false;
                    self.panel_route = r;
                    self.route = r;
                } else if self.panel_visible && self.panel_route == r {
                    self.panel_visible = false;
                } else {
                    self.panel_visible = true;
                    self.panel_route = r;
                    self.route = r;
                }
            }
            TrackSelected(id) => {
                tracing::debug!("Sidebar: 音轨选择 id={}", id);
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
                    can_delete: true,
                    is_muted: false,
                    is_onion_skin_on: true,
                });
                self.selected_track = new_id;
                self.add_track_menu_open = false;
            }
            AddTrackMenuToggled => {
                self.add_track_menu_open = !self.add_track_menu_open;
            }
            ResizeDragStarted(_) => {
                self.is_resizing = true;
            }
            ResizeDragged(_) => {
                // 拖拽中的位置更新由 Host 通过 update_resize_position 处理
            }
            ResizeDragEnded => {
                self.is_resizing = false;
            }
            TrackScrolled(offset) => {
                self.track_scroll_offset = offset;
            }
        }
        // 最终保护：音轨总览模式下强制关闭面板
        if self.route == Route::Arrangement {
            self.panel_visible = false;
        }

        // 当面板可见性变化或路由变化时，都需要重新渲染
        self.panel_visible != prev_visible || self.route != prev_route
    }

    /// 检查是否正在调整大小
    pub fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    /// 开始调整大小，记录起始鼠标 X 坐标
    pub fn start_resize(&mut self, cursor_x: f32) {
        self.is_resizing = true;
        self.resize_start_x = cursor_x;
        self.resize_start_width = self.panel_width;
    }

    /// 更新拖拽位置（从外部传入当前鼠标 X 坐标）
    pub fn update_resize_position(&mut self, cursor_x: f32) {
        if self.is_resizing {
            let delta_x = cursor_x - self.resize_start_x;
            let new_width = self.resize_start_width + delta_x;
            self.panel_width = new_width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        }
    }

    /// 结束调整大小
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Sidebar {
    /// 检查当前是否为音轨总览路由
    pub fn is_arrangement_route(&self) -> bool {
        self.route == Route::Arrangement
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
                is_conductor: *track_idx == 0,
                can_delete: *track_idx != 0,
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
        // 仅在非音轨总览模式下打开面板，确保音轨在面板中可见
        if self.route != Route::Arrangement {
            self.panel_visible = true;
        }
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
