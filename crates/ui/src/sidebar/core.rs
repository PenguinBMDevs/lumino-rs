use iced_core::Color;
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

// ─── 分组系统 ───

/// 侧边栏分组 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupId {
    /// 钢琴卷帘组（红色）
    PianoRoll,
    /// 瀑布流播放器组（黄色）
    Waterfall,
    /// 渲染组（蓝色）
    Renderer,
}

impl GroupId {
    /// 父按钮灯条颜色（硬编码）
    pub fn parent_color(&self) -> Color {
        match self {
            GroupId::PianoRoll => Color::from_rgb(0.85, 0.15, 0.15),
            GroupId::Waterfall => Color::from_rgb(0.85, 0.75, 0.10),
            GroupId::Renderer => Color::from_rgb(0.15, 0.45, 0.85),
        }
    }

    /// 子按钮灯条颜色（比父按钮浅）
    pub fn child_color(&self) -> Color {
        match self {
            GroupId::PianoRoll => Color::from_rgb(0.65, 0.35, 0.35),
            GroupId::Waterfall => Color::from_rgb(0.65, 0.58, 0.30),
            GroupId::Renderer => Color::from_rgb(0.35, 0.55, 0.65),
        }
    }

    pub fn tooltip(&self, lang: Language) -> &'static str {
        match self {
            GroupId::PianoRoll => match lang {
                Language::ZhCn => "钢琴卷帘组",
                Language::EnUs => "Piano Roll",
            },
            GroupId::Waterfall => match lang {
                Language::ZhCn => "瀑布流播放器",
                Language::EnUs => "Waterfall Player",
            },
            GroupId::Renderer => match lang {
                Language::ZhCn => "渲染器",
                Language::EnUs => "Renderer",
            },
        }
    }
}

/// 分组子按钮状态（切换分组时保存/恢复）
#[derive(Debug, Clone)]
pub struct GroupSubState {
    pub panel_visible: bool,
    pub panel_route: Route,
    pub automation_panel_visible: bool,
}

impl Default for GroupSubState {
    fn default() -> Self {
        Self {
            panel_visible: false,
            panel_route: Route::File,
            automation_panel_visible: false,
        }
    }
}

// ─── 路由 ───

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
    /// 组父按钮（定义分组，带颜色指示）
    GroupParent {
        group: GroupId,
        icon: icon::Icon,
    },
    /// 路由项（可关联到某个组作为子按钮）
    Item {
        route: Route,
        icon: icon::Icon,
        group: Option<GroupId>,
    },
    Space,
}

pub const ROUTES: [RouteConfig; 6] = [
    // ── 钢琴卷帘组（红色） ──
    RouteConfig::GroupParent {
        group: GroupId::PianoRoll,
        icon: icon::Keys,
    },
    RouteConfig::Item {
        route: Route::File,
        icon: icon::FolderTree,
        group: Some(GroupId::PianoRoll),
    },
    RouteConfig::Item {
        route: Route::Automation,
        icon: icon::WaveForm,
        group: Some(GroupId::PianoRoll),
    },
    // ── 瀑布流播放器组（黄色） ──
    RouteConfig::GroupParent {
        group: GroupId::Waterfall,
        icon: icon::PlayCircle,
    },
    // ── 渲染组（蓝色） ──
    RouteConfig::GroupParent {
        group: GroupId::Renderer,
        icon: icon::Download,
    },
    // ── 弹性空间 ──
    RouteConfig::Space,
];

// ─── 音轨数据 ───

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub is_conductor: bool,
    pub can_delete: bool,
    pub is_muted: bool,
}

// ─── Sidebar 主结构 ───

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
    // ── 分组状态 ──
    /// 当前激活的分组（None = 无分组激活）
    pub active_group: Option<GroupId>,
    /// 钢琴卷帘组的子按钮保存状态
    pub piano_roll_sub_state: GroupSubState,
    /// 渲染组的子按钮保存状态
    pub renderer_sub_state: GroupSubState,
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
            active_group: Some(GroupId::PianoRoll),
            piano_roll_sub_state: GroupSubState::default(),
            renderer_sub_state: GroupSubState::default(),
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
