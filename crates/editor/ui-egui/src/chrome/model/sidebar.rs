//! 左侧栏 / 右侧栏的只读模型。

use egui::Color32;

use super::RightSidebarPanelModel;

/// 路由栏分组（灯条颜色来源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarGroup {
    /// 钢琴卷帘组（红）。
    PianoRoll,
    /// 工程走带组（绿）。
    Project,
    /// 播放器组（黄）。
    Waterfall,
    /// 渲染组（蓝）。
    Renderer,
}

impl SidebarGroup {
    /// 父按钮灯条颜色（iced `GroupId::parent_color`）。
    pub fn parent_color(self) -> Color32 {
        match self {
            Self::PianoRoll => Color32::from_rgb(0xD9, 0x26, 0x26),
            Self::Project => Color32::from_rgb(0x26, 0xBF, 0x59),
            Self::Waterfall => Color32::from_rgb(0xD9, 0xBF, 0x1A),
            Self::Renderer => Color32::from_rgb(0x26, 0x73, 0xD9),
        }
    }

    /// 子按钮灯条颜色（iced `GroupId::child_color`）。
    pub fn child_color(self) -> Color32 {
        match self {
            Self::PianoRoll => Color32::from_rgb(0xA6, 0x59, 0x59),
            Self::Project => Color32::from_rgb(0x59, 0xA6, 0x73),
            Self::Waterfall => Color32::from_rgb(0xA6, 0x94, 0x4D),
            Self::Renderer => Color32::from_rgb(0x59, 0x8C, 0xA6),
        }
    }
}

/// 卷帘方向按钮。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollBar {
    /// 横向卷帘。
    Horizontal,
    /// 纵向卷帘。
    Vertical,
}

/// 左侧栏面板内容（与 iced `panel::view` 的路由分支一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarPanel {
    /// 音轨列表 + 添加按钮（`Route::File`）。
    #[default]
    Tracks,
    /// 仅添加按钮（`Route::Arrangement`）。
    AddTrackOnly,
    /// 空白（其余路由）。
    Blank,
}

/// 左侧栏的一条音轨（只读）。
#[derive(Debug, Clone)]
pub struct SidebarTrack {
    /// 音轨编号（`A01`）；指挥轨也保留字段，渲染时显示图标。
    pub label: String,
    /// 音轨名称。
    pub name: String,
    /// 自定义颜色（`None` = 默认）。
    pub color: Option<Color32>,
    /// 是否选中。
    pub selected: bool,
    /// 是否指挥轨。
    pub conductor: bool,
    /// 是否独奏。
    pub soloed: bool,
}

/// 左侧栏（路由栏 48 + 面板 `panel_width`）的只读状态。
#[derive(Debug, Clone)]
pub struct SidebarModel {
    /// 面板是否可见。
    pub panel_visible: bool,
    /// 面板宽度（含 6px 拖拽手柄）。
    pub panel_width: f32,
    /// 钢琴卷帘是否可见（决定 PianoRoll 组父按钮点亮）。
    pub piano_roll_visible: bool,
    /// 是否处于瀑布流模式（决定 Waterfall 组父按钮点亮）。
    pub waterfall: bool,
    /// 当前激活分组。
    pub active_group: Option<SidebarGroup>,
    /// 自动化面板是否可见。
    pub automation_panel_visible: bool,
    /// 音频导出面板是否可见。
    pub audio_export_visible: bool,
    /// 视频导出面板是否可见。
    pub video_export_visible: bool,
    /// 「音轨列表」子按钮是否点亮（`panel_visible && route == File`）。
    pub file_active: bool,
    /// 是否渲染卷帘方向按钮。
    pub roll_bar_visible: bool,
    /// 卷帘方向激活项。
    pub roll_bar_active: Option<RollBar>,
    /// 混音台是否打开。
    pub mixer_open: bool,
    /// 面板内容。
    pub panel: SidebarPanel,
    /// 音轨列表。
    pub tracks: Vec<SidebarTrack>,
    /// 「音轨列表」标题。
    pub track_list_title: String,
    /// 「添加音轨」文本。
    pub add_track_label: String,
}

impl SidebarModel {
    /// 左侧栏总宽度（路由栏 48 + 可见面板宽度）。
    pub fn total_width(&self) -> f32 {
        48.0 + if self.panel_visible {
            self.panel_width
        } else {
            0.0
        }
    }
}

impl Default for SidebarModel {
    fn default() -> Self {
        Self {
            panel_visible: true,
            panel_width: 200.0,
            piano_roll_visible: true,
            waterfall: false,
            active_group: Some(SidebarGroup::PianoRoll),
            automation_panel_visible: false,
            audio_export_visible: false,
            video_export_visible: false,
            file_active: true,
            roll_bar_visible: true,
            roll_bar_active: Some(RollBar::Horizontal),
            mixer_open: false,
            panel: SidebarPanel::Tracks,
            tracks: vec![
                SidebarTrack {
                    label: "Conductor".to_owned(),
                    name: "Conductor".to_owned(),
                    color: None,
                    selected: true,
                    conductor: true,
                    soloed: false,
                },
                SidebarTrack {
                    label: "A01".to_owned(),
                    name: "Setup".to_owned(),
                    color: None,
                    selected: false,
                    conductor: false,
                    soloed: false,
                },
            ],
            track_list_title: "音轨列表".to_owned(),
            add_track_label: "添加音轨".to_owned(),
        }
    }
}

/// 右侧栏面板（与 iced `RightSidebarPanel` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightSidebarPanelKind {
    /// 图片转 MIDI。
    ImageToMidi,
    /// 素材库。
    Materials,
    /// 钢琴瀑布流。
    PianoWaterfall,
}

/// 右侧栏（48px 图标列 + 可选面板）的只读状态。
#[derive(Debug, Clone, Default)]
pub struct RightSidebarModel {
    /// 面板是否可见（仅影响面板区域，图标列常驻）。
    pub panel_visible: bool,
    /// 面板宽度（含 6px 拖拽手柄）。
    pub panel_width: f32,
    /// 点亮的图标（`None` = 三个按钮均未点亮）。
    pub active: Option<RightSidebarPanelKind>,
    /// 图标列顶边（标题栏 + 工具栏高度）。
    pub top_inset: f32,
    /// 是否正在拖拽调整面板宽度（决定手柄底色）。
    pub resizing: bool,
    /// 已接管的面板内容；`None` = 内容仍由 iced 绘制（素材库 / 瀑布流）。
    pub content: Option<RightSidebarPanelModel>,
}

impl RightSidebarModel {
    /// 图标列宽度（固定 48）。
    pub const fn bar_width(&self) -> f32 {
        48.0
    }

    /// 右侧栏总宽度（面板 + 图标列）。
    pub fn total_width(&self) -> f32 {
        self.bar_width()
            + if self.panel_visible {
                self.panel_width
            } else {
                0.0
            }
    }
}
