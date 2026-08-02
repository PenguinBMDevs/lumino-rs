use crate::resources::icon;
use crate::sidebar::event_browser;
use iced_core::Color;

pub use lumino_ui_core::sidebar_event::{GroupId, Route};

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

pub const ROUTES: [RouteConfig; 10] = [
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
    RouteConfig::Item {
        route: Route::EventList,
        icon: icon::EventList,
        group: Some(GroupId::PianoRoll),
    },
    // ── 工程走带组（绿色） ──
    RouteConfig::GroupParent {
        group: GroupId::Project,
        icon: icon::Arrangement,
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
    RouteConfig::Item {
        route: Route::VideoExport,
        icon: icon::VideoCamera,
        group: Some(GroupId::Renderer),
    },
    RouteConfig::Item {
        route: Route::AudioExport,
        icon: icon::MusicNote,
        group: Some(GroupId::Renderer),
    },
    // ── 弹性空间 ──
    RouteConfig::Space,
];

// ─── 音轨数据 ───

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    pub name: String,
    /// MIDI 端口（0-25 映射到 A-Z，与 yinhe 一致）
    pub port: u8,
    /// MIDI 通道（0-15）
    pub channel: u8,
    /// 显示标签：`{端口字母}{通道号+1:02}`，如 A01（port=0, ch=0）
    pub display_label: String,
    pub is_conductor: bool,
    pub can_delete: bool,
    pub is_muted: bool,
    pub is_soloed: bool,
    /// 选项卡颜色（None 表示使用默认颜色）
    pub color: Option<Color>,
}

/// 音轨选项卡右键菜单状态
#[derive(Debug, Clone, Default)]
pub struct TrackContextMenuState {
    /// 当前菜单关联的音轨 ID（None 表示菜单未打开）
    pub target_track_id: Option<usize>,
}

// ─── Sidebar 主结构 ───

pub struct Sidebar {
    pub route: Route,
    pub(crate) panel_visible: bool,
    pub(crate) panel_route: Route,
    pub tracks: Vec<Track>,
    pub selected_track: usize,
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
    /// 工程走带组的子按钮保存状态
    pub project_sub_state: GroupSubState,
    /// 渲染组的子按钮保存状态
    pub renderer_sub_state: GroupSubState,
    /// 音频渲染面板是否可见（在主界面钢琴卷帘区域显示）
    pub audio_export_visible: bool,
    /// 视频渲染面板是否可见（在主界面钢琴卷帘区域显示）
    pub video_export_visible: bool,
    /// 音轨选项卡右键菜单状态
    pub track_context_menu: TrackContextMenuState,
    /// 正在重命名的音轨（音轨 ID，当前输入值）
    pub renaming_track: Option<(usize, String)>,
    /// 正在选择颜色的音轨 ID
    pub color_picking_track: Option<usize>,
    /// 事件浏览器状态
    pub event_browser_state: event_browser::EventBrowserState,
    /// 事件列表上下文菜单当前关联的 tick（None 表示未打开）
    pub event_list_context_menu_tick: Option<u32>,
    /// 事件列表待应用到 editor 的操作（由 Root 在 update 后消费）
    pub pending_event_list_action: Option<event_browser::EventListAction>,
    /// 事件列表 popup 待解析的原始编辑请求（由 Root 在 update 后消费）
    pub pending_event_list_edit: Option<(event_browser::EditRequest, String)>,
    /// 事件列表垂直滚动偏移
    pub event_list_scroll_y: f32,
    /// 事件列表可视区域高度（用于虚拟滚动）
    pub event_list_viewport_height: f32,
    /// 单调递增的音轨 ID 计数器（删除后复用 ID 会导致选中冲突）
    pub(crate) next_track_id: usize,
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
                    port: 0,
                    channel: 0,
                    display_label: "A01".to_string(),
                    is_conductor: true,
                    can_delete: false,
                    is_muted: false,
                    is_soloed: false,
                    color: None,
                },
                Track {
                    id: 1,
                    name: "Setup".to_string(),
                    port: 0,
                    channel: 0,
                    display_label: "A01".to_string(),
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                    is_soloed: false,
                    color: None,
                },
            ],
            selected_track: 0,
            panel_width: DEFAULT_PANEL_WIDTH,
            is_resizing: false,
            resize_start_x: 0.0,
            resize_start_width: DEFAULT_PANEL_WIDTH,
            automation_panel_visible: false,
            piano_roll_visible: true,
            active_group: Some(GroupId::PianoRoll),
            piano_roll_sub_state: GroupSubState::default(),
            project_sub_state: GroupSubState::default(),
            renderer_sub_state: GroupSubState::default(),
            audio_export_visible: false,
            video_export_visible: false,
            track_context_menu: TrackContextMenuState::default(),
            renaming_track: None,
            color_picking_track: None,
            event_browser_state: event_browser::EventBrowserState::default(),
            event_list_context_menu_tick: None,
            pending_event_list_action: None,
            pending_event_list_edit: None,
            event_list_scroll_y: 0.0,
            event_list_viewport_height: 0.0,
            next_track_id: 2,
        }
    }

    /// 检查当前是否为音轨总览路由
    pub fn is_arrangement_route(&self) -> bool {
        self.route == Route::Arrangement
    }

    /// yinhe 风格端口字母：port 0→'A', 1→'B', ..., 25→'Z'，超限为 '?'
    fn port_letter(port: u8) -> char {
        if port < 26 {
            (b'A' + port) as char
        } else {
            '?'
        }
    }

    /// yinhe 风格音轨标签：`{端口字母}{通道号+1:02}`
    /// 标签是纯通道指示器，同一端口/通道的多个音轨共享相同标签。
    fn track_label(port: u8, channel: u8) -> String {
        format!("{}{:02}", Self::port_letter(port), channel + 1)
    }

    /// 从 MIDI 数据更新音轨列表（按 port→channel→id 排序，同端口按通道号排列）
    /// 排序键：port（端口字母 A→Z），channel（通道号 01→16），id（稳定排序保序）
    /// track_infos: (track_index, track_name, note_count, channel, port)
    pub fn update_tracks_from_midi(
        &mut self,
        track_infos: &[(usize, Option<String>, u64, u8, u8)],
    ) {
        tracing::info!("update_tracks_from_midi: {} tracks", track_infos.len());
        self.tracks.clear();

        for (idx, (track_idx, name, _note_count, ch, port)) in track_infos.iter().enumerate() {
            let track_name = name.as_deref().unwrap_or("Unknown");
            let label = Self::track_label(*port, *ch);
            tracing::debug!(
                "  track {}: id={}, name={}, port={}, channel={}, label={}",
                idx,
                track_idx,
                track_name,
                port,
                ch,
                label
            );
            self.tracks.push(Track {
                id: *track_idx,
                name: track_name.to_string(),
                port: *port,
                channel: *ch,
                display_label: label,
                is_conductor: *track_idx == 0,
                can_delete: *track_idx != 0,
                is_muted: false,
                is_soloed: false,
                color: None,
            });
        }

        // 按 port→channel→id 排序：同一端口的音轨按通道号排列
        // 端口 A→Z，通道 01→16，id 保序
        self.tracks.sort_by_key(|t| (t.port, t.channel, t.id));

        // 如果有音轨，默认选择第一个
        if !self.tracks.is_empty() {
            self.selected_track = self.tracks[0].id;
            tracing::info!("default selected_track = {}", self.selected_track);
        }

        // 同步 next_track_id
        let max_id = self.tracks.iter().map(|track| track.id).max().unwrap_or(0);
        self.next_track_id = self.next_track_id.max(max_id + 1);
    }

    /// 设置当前选中的音轨（默认强制打开面板，供测试使用）
    #[cfg(test)]
    pub fn set_selected_track(&mut self, track_idx: usize) {
        self.set_selected_track_with_panel(track_idx, true);
    }

    /// 设置当前选中的音轨（可控制是否强制打开面板）
    ///
    /// `open_panel` 为 `true` 时，在非 Arrangement 模式下强制打开面板（用户手动选轨）；
    /// 为 `false` 时只刷新数据，不改变面板可见性（MIDI 加载等程序化操作）。
    pub fn set_selected_track_with_panel(&mut self, track_idx: usize, open_panel: bool) {
        self.selected_track = track_idx;
        if open_panel && self.route != Route::Arrangement {
            self.panel_visible = true;
        }
    }
    /// 取出并清空待执行的 editor 数据操作。
    pub fn take_event_list_action(&mut self) -> Option<event_browser::EventListAction> {
        self.pending_event_list_action.take()
    }

    /// 取出并清空待解析的 popup 编辑请求。
    pub fn take_event_list_edit(&mut self) -> Option<(event_browser::EditRequest, String)> {
        self.pending_event_list_edit.take()
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
