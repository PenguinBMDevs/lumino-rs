//! 侧边栏核心数据类型
//!
//! 包含常量、分组系统配置（`GroupSubState` / `RouteConfig` / `ROUTES`）、
//! 音轨元数据（`Track`）与各类上下文菜单状态。

use crate::resources::icon;
use iced_core::Color;

use super::{GroupId, Route};

/// 路由栏宽度（固定）
pub const ROUTE_BAR_WIDTH: f32 = 48.0;
/// 面板默认宽度
pub const DEFAULT_PANEL_WIDTH: f32 = 200.0;
/// 面板最小宽度
pub const MIN_PANEL_WIDTH: f32 = 150.0;
/// 面板最大宽度
pub const MAX_PANEL_WIDTH: f32 = 900.0;
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

/// 路由配置项（路由栏中的组父按钮或路由项）
#[derive(Debug, Clone)]
pub enum RouteConfig {
    /// 组父按钮（定义分组，带颜色指示）
    GroupParent {
        /// 分组标识
        group: GroupId,
        /// 图标
        icon: icon::Icon,
    },
    /// 路由项（可关联到某个组作为子按钮）
    Item {
        /// 路由
        route: Route,
        /// 图标
        icon: icon::Icon,
        /// 所属分组（None 表示不属于任何组）
        group: Option<GroupId>,
    },
    /// 弹性间距
    Space,
}

/// 路由栏的全部路由配置（9 项）
pub const ROUTES: [RouteConfig; 9] = [
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
    // ── 工程走带组（绿色） ──
    RouteConfig::GroupParent {
        group: GroupId::Project,
        icon: icon::Arrangement,
    },
    // ── 播放器组（黄色） ──
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

/// 音轨元数据
#[derive(Debug, Clone)]
pub struct Track {
    /// 音轨 ID
    pub id: usize,
    /// 音轨名称
    pub name: String,
    /// MIDI 端口（0-25 映射到 A-Z，与 yinhe 一致）
    pub port: u8,
    /// MIDI 通道（0-15）
    pub channel: u8,
    /// 显示标签：`{端口字母}{通道号+1:02}`，如 A01（port=0, ch=0）
    pub display_label: String,
    /// 是否为指挥轨
    pub is_conductor: bool,
    /// 是否允许删除
    pub can_delete: bool,
    /// 是否静音
    pub is_muted: bool,
    /// 是否 Solo
    pub is_soloed: bool,
    /// 选项卡颜色（None 表示使用默认颜色）
    pub color: Option<Color>,
}

/// 待删除音轨的元数据缓存
///
/// 用户在音轨选项卡右键菜单点击"删除"时，sidebar 立即从 `tracks` 中
/// 移除入口并设置 `pending_track_deletion = Some(id)`。由于移除后无法再
/// 从 `tracks` 中查询音轨元数据（名称/port/channel/原位置索引），
/// 这里在移除前缓存一份，供 Root 构造 `TrackDeletionPayload` 时使用。
#[derive(Debug, Clone)]
pub struct PendingTrackDeletionMeta {
    pub track_name: String,
    pub port: u8,
    pub channel: u8,
    /// 在 sidebar.tracks 中的原始位置索引（移除前的位置）
    pub original_index: usize,
}

/// 音轨选项卡右键菜单状态
#[derive(Debug, Clone, Default)]
pub struct TrackContextMenuState {
    /// 当前菜单关联的音轨 ID（None 表示菜单未打开）
    pub target_track_id: Option<usize>,
}

/// 音轨列表面板空白区域右键菜单状态
#[derive(Debug, Clone, Default)]
pub struct PanelContextMenuState {
    /// 当前菜单是否打开
    pub is_open: bool,
    /// 菜单打开时的鼠标位置（窗口逻辑坐标，用于定位菜单）
    pub mouse_pos: Option<(f32, f32)>,
}

impl PanelContextMenuState {
    /// 清除菜单状态
    pub fn reset(&mut self) {
        self.is_open = false;
        self.mouse_pos = None;
    }
}
