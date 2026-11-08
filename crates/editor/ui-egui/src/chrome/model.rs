//! 生产状态 → egui 外壳的只读视图模型。
//!
//! `lumino-ui-egui` 不能依赖 `lumino-ui`（会引入 iced），因此由生产侧
//! （`Root::egui_chrome_model`）把状态填进这里的纯数据类型，egui 只负责绘制。
//! 这样同一份渲染代码既能被离线场景复用，也能被真实窗口叠加层复用。

use egui::Color32;

mod right_panel;
mod settings_audio;
mod settings_cloud;
mod settings_page;
mod settings_ui;
mod tool_panel;
mod toolbar;
pub use right_panel::*;
pub use settings_audio::*;
pub use settings_cloud::*;
pub use settings_page::*;
pub use settings_ui::*;
pub use tool_panel::*;
pub use toolbar::*;

use super::piano_roll::PianoRollView;

/// 主窗口外壳的只读快照。
#[derive(Debug, Clone, Default)]
pub struct ChromeModel {
    /// 独立对话框整窗（设置 / 保存确认 …）；`Some` 时只画对话框，不画任何 band。
    pub dialog: Option<DialogModel>,
    /// 标题栏；`None` = 使用系统原生标题栏，egui 不接管。
    pub titlebar: Option<TitlebarModel>,
    /// 工具栏；`None` = 不接管。
    pub toolbar: Option<ToolbarModel>,
    /// 悬浮绘制工具条（含锚定面板）；`None` = 未打开或不接管。
    pub tool_panel: Option<ToolPanelModel>,
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
    /// 工程设置（`DialogType::ProjectSettings`，系统原生标题栏，整窗内容）。
    ProjectSettings(Box<ProjectSettingsModel>),
    /// 撤销历史（`DialogType::History`，只读时间线 + 跳转）。
    History(Box<HistoryModel>),
}

/// 工程设置对话框：全部文案与只读值由生产侧预计算。
#[derive(Debug, Clone, Default)]
pub struct ProjectSettingsModel {
    /// 顶部是否有一条 30px 自制标题栏（`!use_native_titlebar` 时 iced 绘制）。
    ///
    /// egui 只接管标题栏以下的内容区；标题栏（红绿灯 / 拖动）仍由 iced 绘制，
    /// 避免遮掉窗口控制按钮。
    pub titlebar: bool,
    /// 窗口标题「工程信息设置」。
    pub title: String,
    /// 项目名称标签 / 值 / 空值占位。
    pub name_label: String,
    pub name: String,
    pub name_hint: String,
    /// BPM 标签 / 值（输入框右侧固定「BPM」单位）。
    pub bpm_label: String,
    pub bpm: String,
    /// 版权信息标签 / 值 / 占位。
    pub copyright_label: String,
    pub copyright: String,
    pub copyright_hint: String,
    /// 作者标签 / 值 / 占位。
    pub author_label: String,
    pub author: String,
    pub author_hint: String,
    /// 拍号标签 / 分子 / 分母。
    pub time_signature_label: String,
    pub numerator: String,
    pub denominator: String,
    /// 创建日期标签 / 值。
    pub created_label: String,
    pub created: String,
    /// 累计创作时间标签 / 值。
    pub editing_time_label: String,
    pub editing_time: String,
    /// 音符总量（仅 `NoteCountDisplay::ProjectSettings` 时出现）。
    pub note_count: Option<ProjectNoteCountModel>,
    /// 按钮文案。
    pub ok: String,
    pub cancel: String,
}

/// 工程设置里的「音符总量」只读行。
#[derive(Debug, Clone, Default)]
pub struct ProjectNoteCountModel {
    /// 标签（「音符总量」）。
    pub label: String,
    /// 已格式化的数值（千位分隔）。
    pub value: String,
}

/// 撤销历史对话框（只读时间线 + 跳转）。
#[derive(Debug, Clone, Default)]
pub struct HistoryModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「撤销历史」。
    pub title: String,
    /// 提示语。
    pub hint: String,
    /// 行（初始状态 + 每个步骤，最老在上）。
    pub rows: Vec<HistoryRowModel>,
    /// 底部计数「current / total」。
    pub counter: String,
    /// 关闭按钮。
    pub close: String,
}

/// 历史面板的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRowModel {
    /// 跳转目标（0 = 初始状态）。
    pub index: usize,
    /// 行文案（已本地化）。
    pub label: String,
    /// 是否为当前游标所在行。
    pub current: bool,
    /// 是否为「未来」步骤（浅色）。
    pub future: bool,
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
    /// 当前页内容（未迁移的页面整窗交回 iced，不在此建模）。
    pub page: SettingsPageModel,
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
    /// 主按钮悬停底色（`primary.strong`）。
    pub primary_hover: Color32,
    /// 主按钮边框（`primary.weak`）。
    pub primary_border: Color32,
    /// 次按钮底色（`background.weak`）。
    pub secondary: Color32,
    /// 次按钮悬停底色（`background.strong`）。
    pub secondary_hover: Color32,
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
        primary_hover: Color32::from_rgb(0x00, 0xA3, 0xBD),
        primary_border: Color32::from_rgb(0x28, 0x9D, 0xB4),
        secondary: Color32::from_rgb(0x39, 0x40, 0x63),
        secondary_hover: Color32::from_rgb(0x44, 0x4D, 0x79),
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

/// 自定义标题栏状态。
#[derive(Debug, Clone, Default)]
pub struct TitlebarModel {
    /// 窗口是否聚焦（决定标题栏底色：`background.neutral` / `background.weaker`）。
    pub focused: bool,
    /// 菜单标签（文件 / 编辑 / 视图 / 帮助，按当前语言）。
    pub menus: Vec<String>,
    /// 可交互菜单树（`None` = 只画标签，交互仍交给 iced）。
    pub menu: Option<MenuBarModel>,
    /// 系统原生标题栏模式：只画 30px 菜单栏（无 logo、窗口控制、拖动区）。
    pub menu_only: bool,
}

/// 标题栏菜单栏（生产 `titlebar::menu` 的数据镜像）。
#[derive(Debug, Clone, Default)]
pub struct MenuBarModel {
    /// 四个主菜单（文件 / 编辑 / 视图 / 帮助）。
    pub menus: Vec<MenuModel>,
}

/// 单个主菜单。
#[derive(Debug, Clone, Default)]
pub struct MenuModel {
    /// 菜单标签。
    pub label: String,
    /// 菜单项。
    pub entries: Vec<MenuEntry>,
}

/// 菜单项（与生产 `titlebar::menu::MenuItem` 一一对应）。
#[derive(Debug, Clone)]
pub enum MenuEntry {
    /// 可点击动作；`action` 是在整棵菜单树里的遍历序号。
    Action {
        /// 本地化显示名。
        label: String,
        /// 是否可点击（生产 `ActionDisabled` 为 `false`）。
        enabled: bool,
        /// 动作序号。
        action: usize,
    },
    /// 分隔线。
    Separator,
    /// 子菜单（当前生产只有一级）。
    Submenu {
        /// 本地化显示名。
        label: String,
        /// 子项。
        entries: Vec<MenuEntry>,
    },
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
