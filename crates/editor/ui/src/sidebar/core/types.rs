//! 侧边栏核心数据类型
//!
//! 包含常量、分组系统配置（`GroupSubState` / `RouteConfig` / `ROUTES`）、
//! 音轨元数据（`Track`）与各类上下文菜单状态。

use crate::resources::icon;
use iced_core::Color;
use std::collections::HashMap;

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

/// 混音台单条音轨的音频域参数。
///
/// - `gain`：线性增益（1.0 = 0 dB），负数按 0 处理。
/// - `pan`：声像，∈ [-1, 1]，0 = 居中（-1 = 全左，1 = 全右）。
///
/// 注意：静音/独奏仍由 `Track.is_muted / is_soloed` 作为单一来源，
/// 此处仅承载增益与声像这两项混音台专有参数，避免污染文档音轨元数据。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripParams {
    pub gain: f32,
    pub pan: f32,
}

impl Default for StripParams {
    fn default() -> Self {
        Self {
            gain: 1.0,
            pan: 0.0,
        }
    }
}

/// 混音台状态：以音轨 ID 为键的增益/声像表。
///
/// 与 `Sidebar.tracks` 一一对应（按音轨 ID 索引）。某音轨缺失条目时
/// 视为默认值（`StripParams::default()`）。
#[derive(Debug, Clone, Default)]
pub struct MixerState {
    pub strips: HashMap<usize, StripParams>,
}

impl MixerState {
    /// 读取某音轨的混音参数（缺失则返回默认）。
    pub fn get(&self, id: usize) -> StripParams {
        self.strips.get(&id).copied().unwrap_or_default()
    }

    /// 设置某音轨增益（保留声像），负数按 0 处理。
    pub fn set_gain(&mut self, id: usize, gain: f32) {
        let mut params = self.get(id);
        params.gain = gain.max(0.0);
        self.strips.insert(id, params);
    }

    /// 设置某音轨声像（保留增益），超出 [-1,1] 自动夹紧。
    pub fn set_pan(&mut self, id: usize, pan: f32) {
        let mut params = self.get(id);
        params.pan = pan.clamp(-1.0, 1.0);
        self.strips.insert(id, params);
    }

    /// 移除某音轨的混音参数（音轨删除时调用，避免孤儿条目）。
    pub fn remove(&mut self, id: usize) {
        self.strips.remove(&id);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_params_default() {
        let s = StripParams::default();
        assert_eq!(s.gain, 1.0);
        assert_eq!(s.pan, 0.0);
    }

    #[test]
    fn test_mixer_get_default_when_absent() {
        let mixer = MixerState::default();
        assert_eq!(mixer.get(42), StripParams::default());
    }

    #[test]
    fn test_mixer_set_gain_clamps_negative() {
        let mut mixer = MixerState::default();
        mixer.set_gain(1, -0.5);
        assert_eq!(mixer.get(1).gain, 0.0);
        mixer.set_gain(1, 2.0);
        assert_eq!(mixer.get(1).gain, 2.0);
    }

    #[test]
    fn test_mixer_set_pan_clamps_range() {
        let mut mixer = MixerState::default();
        mixer.set_pan(2, 5.0);
        assert_eq!(mixer.get(2).pan, 1.0);
        mixer.set_pan(2, -9.0);
        assert_eq!(mixer.get(2).pan, -1.0);
        mixer.set_pan(2, 0.3);
        assert_eq!(mixer.get(2).pan, 0.3);
    }

    #[test]
    fn test_mixer_set_gain_keeps_pan() {
        let mut mixer = MixerState::default();
        mixer.set_pan(3, -0.7);
        mixer.set_gain(3, 0.5);
        let strip = mixer.get(3);
        assert_eq!(strip.gain, 0.5);
        assert_eq!(strip.pan, -0.7);
    }

    #[test]
    fn test_mixer_remove() {
        let mut mixer = MixerState::default();
        mixer.set_gain(7, 0.25);
        assert_eq!(mixer.get(7).gain, 0.25);
        mixer.remove(7);
        assert_eq!(mixer.get(7), StripParams::default());
    }
}
