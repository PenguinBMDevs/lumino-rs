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
    /// 生产音轨 ID（`Event::TrackSelected` 用它，不能用列表下标——重排后会错位）。
    pub id: usize,
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
    /// 是否正在行内重命名（该行由 egui 绘制输入框）。
    pub renaming: bool,
    /// 重命名草稿（仅 `renaming` 时有意义）。
    pub rename_buffer: String,
}

/// 路由栏图标（与生产 `ROUTES` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarRouteIcon {
    /// 钢琴卷帘组父按钮。
    PianoRoll,
    /// 音轨列表子按钮。
    File,
    /// 自动化面板子按钮。
    Automation,
    /// 工程走带组父按钮。
    Project,
    /// 播放器（瀑布流）组父按钮。
    Waterfall,
    /// 渲染组父按钮。
    Renderer,
    /// 视频导出子按钮。
    VideoExport,
    /// 音频导出子按钮。
    AudioExport,
}

/// 路由栏条目（顺序即绘制顺序，与生产 `ROUTES` 同源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarRouteItem {
    /// 图标。
    pub icon: SidebarRouteIcon,
    /// 是否点亮灯条。
    pub active: bool,
}

/// 拖拽排序状态（绘制插入指示线用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarReorderModel {
    /// 被拖拽的音轨 ID。
    pub track_id: usize,
    /// 插入位置（`0..=tracks.len()`，已由生产按 conductor 首位不变量钳制）。
    pub hover_index: Option<usize>,
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
    /// 路由栏条目（生产装配；空 = 走内置推导，仅离线场景）。
    pub route_items: Vec<SidebarRouteItem>,
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
    /// 音轨行右键菜单（`None` = 未打开）。
    pub context_menu: Option<SidebarTrackMenuModel>,
    /// 面板空白右键菜单（`None` = 未打开）。
    pub panel_menu: Option<SidebarPanelMenuModel>,
    /// 音轨取色器（`None` = 未打开）。
    pub color_picker: Option<SidebarColorPickerModel>,
    /// 端口 / 通道选择器（`None` = 未打开）。
    pub numeric_picker: Option<SidebarNumericPickerModel>,
    /// 是否正在拖拽调整面板宽度（决定右侧 6px 手柄底色）。
    pub resizing: bool,
    /// 拖拽排序中（`None` = 未拖拽）；用于绘制插入指示线。
    pub reorder: Option<SidebarReorderModel>,
}

/// 右键菜单条目（图标 + 本地化文案；点击回传下标）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarMenuEntry {
    /// 图标。
    pub icon: crate::icons::Icon,
    /// 悬停提示 / 无障碍文案。
    pub label: String,
}

/// 音轨行右键菜单（顺序即生产 `TrackContextMenuItem` 的顺序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarTrackMenuModel {
    /// 目标音轨 ID。
    pub track_id: usize,
    /// 五个条目：删除 / 重命名 / 设置颜色 / 设置通道 / 设置端口。
    pub entries: Vec<SidebarMenuEntry>,
}

/// 面板空白右键菜单。
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarPanelMenuModel {
    /// 触发位置（窗口逻辑坐标）。
    pub pos: egui::Pos2,
    /// 条目（当前只有「找回删除音轨」）。
    pub entries: Vec<SidebarMenuEntry>,
}

/// 音轨取色器（`None` = 未打开）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarColorPickerModel {
    /// 目标音轨 ID。
    pub track_id: usize,
}

/// 数值选择器种类（端口 / 通道）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarNumericPickerKind {
    /// 端口（0..=15，显示 1..=16）。
    Port,
    /// 通道（0..=15，显示 1..=16）。
    Channel,
}

/// 音轨端口 / 通道选择器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarNumericPickerModel {
    /// 目标音轨 ID。
    pub track_id: usize,
    /// 种类。
    pub kind: SidebarNumericPickerKind,
    /// 当前值（内部 0 基）。
    pub current: u8,
    /// 可选数量（4 的倍数）。
    pub choices: u8,
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
            route_items: Vec::new(),
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
                    id: 0,
                    label: "Conductor".to_owned(),
                    name: "Conductor".to_owned(),
                    color: None,
                    selected: true,
                    conductor: true,
                    soloed: false,
                    renaming: false,
                    rename_buffer: String::new(),
                },
                SidebarTrack {
                    id: 1,
                    label: "A01".to_owned(),
                    name: "Setup".to_owned(),
                    color: None,
                    selected: false,
                    conductor: false,
                    soloed: false,
                    renaming: false,
                    rename_buffer: String::new(),
                },
            ],
            track_list_title: "音轨列表".to_owned(),
            add_track_label: "添加音轨".to_owned(),
            context_menu: None,
            panel_menu: None,
            color_picker: None,
            numeric_picker: None,
            resizing: false,
            reorder: None,
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

/// 右侧栏图标列单项（顺序即自上而下）。
///
/// 文案由生产侧格式化（`ui-egui` 不引 i18n）；`kind` 决定点亮态与回传动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RightSidebarRailItem {
    /// 对应面板。
    pub kind: RightSidebarPanelKind,
    /// 悬停提示文案。
    pub label: String,
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
    /// 图标列按钮（自上而下）。为空时回退到内置的三项顺序（仅为测试便利）。
    pub rail: Vec<RightSidebarRailItem>,
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
