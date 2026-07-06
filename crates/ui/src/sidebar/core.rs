use lumino_core::i18n::Language;

use crate::resources::icon;

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
    Automation,
}

impl Route {
    pub fn tooltip(&self, lang: Language) -> &'static str {
        let t = lumino_core::i18n::main_translations(lang);
        match self {
            Route::File => t.sidebar_file,
            Route::Arrangement => t.sidebar_arrangement,
            Route::Automation => t.sidebar_automation,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RouteConfig {
    Item {
        route: Route,
        icon: icon::Icon,
    },
    /// 独立切换按钮（不绑定 Route，用于钢琴卷帘等开关）
    Toggle {
        icon: icon::Icon,
    },
    /// 瀑布流模式切换按钮（圆圈播放按钮样式）
    WaterfallToggle,
    /// 音频导出按钮
    AudioExport,
    Space,
}

pub const ROUTES: [RouteConfig; 7] = [
    RouteConfig::WaterfallToggle,
    RouteConfig::Toggle { icon: icon::Keys },
    RouteConfig::Item {
        route: Route::File,
        icon: icon::FolderTree,
    },
    RouteConfig::Item {
        route: Route::Arrangement,
        icon: icon::Arrangement,
    },
    RouteConfig::Item {
        route: Route::Automation,
        icon: icon::WaveForm,
    },
    RouteConfig::AudioExport,
    RouteConfig::Space,
];

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub is_conductor: bool,
    pub can_delete: bool,
    pub is_muted: bool,
}

pub struct Sidebar {
    pub route: Route,
    pub(crate) panel_visible: bool,
    pub(crate) panel_route: Route,
    pub tracks: Vec<Track>,
    pub selected_track: usize,
    pub add_track_menu_open: bool,
    /// 面板宽度（默认 200）
    pub panel_width: f32,
    /// 是否正在拖拽调整宽度
    pub(crate) is_resizing: bool,
    /// 拖拽开始时的鼠标 X 坐标
    pub(crate) resize_start_x: f32,
    /// 拖拽开始时的面板宽度
    pub(crate) resize_start_width: f32,
    /// 自动化面板是否可见（独立于路由面板）
    pub automation_panel_visible: bool,
    /// 钢琴卷帘编辑器是否可见（默认打开）
    pub piano_roll_visible: bool,
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
                },
                Track {
                    id: 1,
                    name: "Setup".to_string(),
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                },
            ],
            selected_track: 0,
            add_track_menu_open: false,
            panel_width: DEFAULT_PANEL_WIDTH,
            is_resizing: false,
            resize_start_x: 0.0,
            resize_start_width: DEFAULT_PANEL_WIDTH,
            automation_panel_visible: false,
            piano_roll_visible: true,
        }
    }

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
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
