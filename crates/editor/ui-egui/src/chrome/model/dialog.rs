//! 独立对话框的只读模型（保存确认 / 设置 / 工程设置 / 历史 / 批量编辑）。

use egui::Color32;

use super::{
    BatchEditModel, CloudConnectModel, HelpModel, HistoryModel, MemoryMonitorModel,
    SettingsPageModel,
};

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
    /// 批量编辑（`DialogType::BatchEdit`）。
    BatchEdit(Box<BatchEditModel>),
    /// 变速（`DialogType::SpeedChange`）。
    SpeedChange(Box<SpeedChangeModel>),
    /// 自定义精度（`DialogType::CustomPrecision`）。
    CustomPrecision(Box<CustomPrecisionModel>),
    /// 音频导出进度（`DialogType::ExportProgress`，只读）。
    ExportProgress(Box<ExportProgressModel>),
    /// Help 窗口（`DialogType::Help`）。
    Help(Box<HelpModel>),
    /// 云存储连接面板（`DialogType::CloudConnect`）。
    CloudConnect(Box<CloudConnectModel>),
    /// 内存监控（`DialogType::MemoryMonitor`）。
    MemoryMonitor(Box<MemoryMonitorModel>),
}

/// 导出进度对话框：标题 / 状态行 / 进度 / 百分比。
#[derive(Debug, Clone, Default)]
pub struct ExportProgressModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「音频导出」。
    pub title: String,
    /// 状态行文本（错误信息 / 「导出完成」/ 进度消息）。
    pub message: String,
    /// 状态行语义（决定颜色）。
    pub kind: ExportMessageKind,
    /// 进度 0.0..=1.0。
    pub progress: f32,
    /// 百分比文本（「33.1%」）。
    pub percentage: String,
}

/// 状态行语义。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExportMessageKind {
    /// 普通进度消息。
    #[default]
    Normal,
    /// 导出失败（危险色）。
    Error,
    /// 导出完成（成功色）。
    Completed,
}

/// 自定义精度对话框：三连音数 / 分音符 / 附点 / 除数 / 错误行 / 按钮。
#[derive(Debug, Clone, Default)]
pub struct CustomPrecisionModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 三连音数（默认 3）。
    pub tuplet_count: String,
    /// 分音符时值（默认 4）。
    pub note_value: String,
    /// 「分音符」标签。
    pub note_label: String,
    /// 「附点」标签。
    pub dot_label: String,
    /// 附点选项（4 个）。
    pub dots: Vec<DotOptionModel>,
    /// 「除以」标签。
    pub divide_label: String,
    /// 除数（默认 1）。
    pub divisor: String,
    /// 上一次提交失败的原因。
    pub error: Option<String>,
    /// 确定按钮。
    pub ok: String,
    /// 取消按钮。
    pub cancel: String,
}

/// 附点分段按钮。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotOptionModel {
    /// 对应生产 `DotType`。
    pub dot: lumino_core::DotType,
    /// 已本地化的选项名。
    pub label: String,
    /// 是否选中。
    pub selected: bool,
}

/// 变速对话框：标题 / 倍率行 / 提示 / 按钮。
#[derive(Debug, Clone, Default)]
pub struct SpeedChangeModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「变速」。
    pub title: String,
    /// 倍率标签（「倍率(S):」）。
    pub factor_label: String,
    /// 倍率输入框占位（「如: 0.5 或 1/3」）。
    pub factor_hint: String,
    /// 当前倍率文本。
    pub factor: String,
    /// 提示（「支持小数 (0.5) 或分数 (1/3)」）。
    pub hint: String,
    /// 确定按钮。
    pub ok: String,
    /// 取消按钮。
    pub cancel: String,
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
