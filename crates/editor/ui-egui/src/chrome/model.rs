//! 生产状态 → egui 外壳的只读视图模型。
//!
//! `lumino-ui-egui` 不能依赖 `lumino-ui`（会引入 iced），因此由生产侧
//! （`Root::egui_chrome_model`）把状态填进这里的纯数据类型，egui 只负责绘制。
//! 这样同一份渲染代码既能被离线场景复用，也能被真实窗口叠加层复用。

use egui::Color32;

use super::piano_roll::PianoRollView;
use lumino_core::Tool;
use lumino_core::storage::config::AutoScrollMode;

/// 主窗口外壳的只读快照。
#[derive(Debug, Clone, Default)]
pub struct ChromeModel {
    /// 独立对话框整窗（设置 / 保存确认 …）；`Some` 时只画对话框，不画任何 band。
    pub dialog: Option<DialogModel>,
    /// 标题栏；`None` = 使用系统原生标题栏，egui 不接管。
    pub titlebar: Option<TitlebarModel>,
    /// 工具栏；`None` = 不接管。
    pub toolbar: Option<ToolbarModel>,
    /// 左侧栏；`None` = 不接管（拖拽 / 重命名 / 菜单等瞬态）。
    pub sidebar: Option<SidebarModel>,
    /// 右侧栏图标列；`None` = 不接管（当前视图不渲染右侧栏）。
    pub right_sidebar: Option<RightSidebarModel>,
    /// 钢琴卷帘骨架；`None` = 不接管（纵向卷帘 / 循环 / 拍号变化 / 洋葱皮 / 协作光标）。
    pub piano_roll: Option<PianoRollView>,
    /// 状态栏。
    pub statusbar: StatusbarModel,
}

/// 独立对话框窗口的只读模型。
#[derive(Debug, Clone)]
pub enum DialogModel {
    /// 未保存更改确认（`DialogType::SaveConfirm`）。
    SaveConfirm(SaveConfirmModel),
    /// 设置对话框「常规」页（`DialogType::Settings`，仅 `selected == 0`）。
    ///
    /// 装箱：`SettingsSnapshot` 含全部文案与调色板，比确认对话框大一个数量级。
    Settings(Box<SettingsSnapshot>),
}

/// 设置对话框调色板：由生产 iced `extended_palette` 映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsPalette {
    /// 自制标题栏（`background.neutral`）。
    pub titlebar: Color32,
    /// 内容底色（`background.base`）。
    pub background: Color32,
    /// 容器底色（`background.weakest`）。
    pub panel: Color32,
    /// 菜单底色（`background.weak`）。
    pub toolbar: Color32,
    /// 卡片底色。
    pub card: Color32,
    /// 菜单/卡片边框。
    pub border: Color32,
    /// 输入控件边框（`background.strong`）。
    pub input_border: Color32,
    /// 正文（`background.base.text`）。
    pub text: Color32,
    /// 次级文字。
    pub text_dim: Color32,
    /// 强调色（`primary.base`）。
    pub accent: Color32,
    /// 强调色强（`primary.strong`）。
    pub accent_strong: Color32,
}

impl Default for SettingsPalette {
    fn default() -> Self {
        Self::DARK
    }
}

impl SettingsPalette {
    /// 深色主题（Tokyo Night Storm），与既有像素基线一致。
    pub const DARK: Self = Self {
        titlebar: Color32::from_rgb(0x3F, 0x46, 0x6E),
        background: Color32::from_rgb(0x24, 0x28, 0x3B),
        panel: Color32::from_rgb(0x2A, 0x2F, 0x47),
        toolbar: Color32::from_rgb(0x39, 0x40, 0x63),
        card: Color32::from_rgb(0x24, 0x28, 0x3B),
        border: Color32::from_rgb(0x3F, 0x45, 0x5C),
        input_border: Color32::from_rgb(0x44, 0x4D, 0x79),
        text: Color32::from_rgb(0xB6, 0xC4, 0xFA),
        text_dim: Color32::from_rgb(0x7A, 0x82, 0xA8),
        accent: Color32::from_rgb(0x2A, 0xC3, 0xDE),
        accent_strong: Color32::from_rgb(0x00, 0xA3, 0xBD),
    };
}

/// 设置对话框的本地化文案（来自生产 `settings_translations`）。
///
/// 渲染层不持有任何语言常量：中/英文都由生产侧填入，换语言只换数据。
#[derive(Debug, Clone, Default)]
pub struct SettingsLabels {
    /// 左侧菜单（顺序与图标列一致：常规 / 音频 / 界面 / 快捷键 / 洋葱皮 /
    /// 调色板 / 编辑 / 云管理 / 兼容性 / 关于）。
    pub menu: Vec<String>,
    /// 页面标题（「常规」）。
    pub title: String,
    /// 橡皮擦行为标签。
    pub eraser_label: String,
    /// 橡皮擦行为选项（两项，与生产 `pick_list` 同序）。
    pub eraser_options: Vec<String>,
    /// 添加音轨行为标签。
    pub track_add_label: String,
    /// 添加音轨行为选项（两项，与生产 `pick_list` 同序）。
    pub track_add_options: Vec<String>,
    /// 日志分区标题。
    pub log_section: String,
    /// 日志保留份数标签。
    pub log_label: String,
    /// 日志输入框占位符（生产 `text_input("10", ...)`）。
    pub log_placeholder: String,
    /// 确认按钮。
    pub confirm: String,
}

/// 设置对话框整窗所需的真实状态（值来自生产 `SettingsPanel`）。
#[derive(Debug, Clone, Default)]
pub struct SettingsSnapshot {
    /// 调色板（深/浅主题）。
    pub palette: SettingsPalette,
    /// 本地化文案。
    pub labels: SettingsLabels,
    /// 当前菜单索引（只接管 0 = 常规页）。
    pub selected: usize,
    /// 橡皮擦行为（本地化后的显示文本）。
    pub eraser: String,
    /// 添加音轨行为（本地化后的显示文本）。
    pub track_add: String,
    /// 日志文件保留份数。
    pub log_retention: String,
}

/// 对话框调色板：由生产 iced `extended_palette` 映射，深/浅主题都能整窗接管。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogPalette {
    /// 自制标题栏底色（`background.neutral`）。
    pub titlebar: Color32,
    /// 内容底色（`background.base`）。
    pub background: Color32,
    /// 正文色（`background.base.text`）。
    pub text: Color32,
    /// 主按钮底色（`primary.base`）。
    pub primary: Color32,
    /// 主按钮边框（`primary.weak`）。
    pub primary_border: Color32,
    /// 次按钮底色（`background.weak`）。
    pub secondary: Color32,
    /// 次按钮边框（`background.weaker`）。
    pub secondary_border: Color32,
}

impl DialogPalette {
    /// 深色主题（Tokyo Night Storm）映射，与既有像素基线一致。
    pub const DARK: Self = Self {
        titlebar: Color32::from_rgb(0x3F, 0x46, 0x6E),
        background: Color32::from_rgb(0x24, 0x28, 0x3B),
        text: Color32::from_rgb(0xB6, 0xC4, 0xFA),
        primary: Color32::from_rgb(0x2A, 0xC3, 0xDE),
        primary_border: Color32::from_rgb(0x28, 0x9D, 0xB4),
        secondary: Color32::from_rgb(0x39, 0x40, 0x63),
        secondary_border: Color32::from_rgb(0x33, 0x39, 0x57),
    };
}

/// 保存确认对话框的文案与标题栏开关。
#[derive(Debug, Clone)]
pub struct SaveConfirmModel {
    /// 是否绘制自制标题栏（系统原生标题栏时为 `false`）。
    pub titlebar: bool,
    /// 调色板（深/浅主题）。
    pub palette: DialogPalette,
    /// 提示语。
    pub message: String,
    /// 主按钮（保存）。
    pub save: String,
    /// 次按钮（放弃更改并关闭）。
    pub close: String,
    /// 次按钮（取消）。
    pub cancel: String,
}

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
#[derive(Debug, Clone, Copy, Default)]
pub struct RightSidebarModel {
    /// 面板是否可见（仅影响面板区域，图标列常驻）。
    pub panel_visible: bool,
    /// 面板宽度（含 6px 拖拽手柄）。
    pub panel_width: f32,
    /// 点亮的图标（`None` = 三个按钮均未点亮）。
    pub active: Option<RightSidebarPanelKind>,
    /// 图标列顶边（标题栏 + 工具栏高度）。
    pub top_inset: f32,
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

/// 工具栏（72px band）的只读状态。
#[derive(Debug, Clone)]
pub struct ToolbarModel {
    /// 工具栏左边界（左侧栏占用宽度，逻辑点）。
    pub left_inset: f32,
    /// BPM 文本（保留 1 位小数）。
    pub bpm: String,
    /// PPQ 文本（形如 `PPQ 1920`）。
    pub ppq: String,
    /// 播放位置「小节.拍」。
    pub position: String,
    /// 播放时间 `mm:ss.cs`。
    pub time: String,
    /// 吸附规格文本（形如 `1/4`）。
    pub snap: String,
    /// 当前工具。
    pub tool: Tool,
    /// 绘制工具面板是否打开（第 5 个选择器的选中态）。
    pub draw_panel_open: bool,
    /// 是否正在播放（播放按钮显示暂停图标）。
    pub playing: bool,
    /// 是否循环播放（循环按钮图标切换）。
    pub looping: bool,
    /// 是否正在录制。
    pub recording: bool,
    /// 洋葱皮开关（`background.strong` 底色表示开启）。
    pub onion: bool,
    /// 自动滚动模式（决定右侧图标）。
    pub auto_scroll: AutoScrollMode,
}

impl Default for ToolbarModel {
    fn default() -> Self {
        Self {
            left_inset: 0.0,
            bpm: "120.0".to_owned(),
            ppq: "PPQ 1920".to_owned(),
            position: "1.1".to_owned(),
            time: "00:00.00".to_owned(),
            snap: "1/4".to_owned(),
            tool: Tool::Pointer,
            draw_panel_open: false,
            playing: false,
            looping: false,
            recording: false,
            onion: false,
            auto_scroll: AutoScrollMode::ScrollingIndicator,
        }
    }
}

/// 自定义标题栏状态。
#[derive(Debug, Clone, Default)]
pub struct TitlebarModel {
    /// 窗口是否聚焦（决定标题栏底色：`background.neutral` / `background.weaker`）。
    pub focused: bool,
    /// 菜单标签（文件 / 编辑 / 视图 / 帮助，按当前语言）。
    pub menus: Vec<String>,
}

/// 状态栏状态。
#[derive(Debug, Clone, Default)]
pub struct StatusbarModel {
    /// 左侧描述文本（悬停提示 / 就绪 / 音符数）。
    pub left: String,
    /// 左侧文本是否用强调色（悬停说明 / FPS 风格）。
    pub left_accent: bool,
    /// CPU 占用百分比；`None` = 尚未采样，整组指标隐藏。
    pub cpu_percent: Option<f32>,
    /// 进程内存（MB）。
    pub memory_mb: f32,
    /// 平滑后的 FPS。
    pub fps: Option<f32>,
    /// 右侧补充文本（生产 `StatusInfo.right_text`）。
    pub right: String,
}

impl StatusbarModel {
    /// 是否显示 CPU / MEM / FPS 指标组（生产：`fps.is_some()`）。
    pub fn has_metrics(&self) -> bool {
        self.fps.is_some()
    }
}
