//! 设置对话框「其余页面」的只读模型。
//!
//! 设置菜单共 10 页：本步迁移快捷键 / 洋葱皮 / 兼容性三页（外加已完成的常规页）；
//! 其余页面仍由 iced 绘制，`SettingsSnapshot::page` 为 `None` 时整窗交回 iced。
//!
//! 全部文案由生产侧按当前语言预计算，`lumino-ui-egui` 不做 i18n。

use super::AudioPageModel;

/// 设置对话框的页面内容。
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SettingsPageModel {
    /// 常规页（橡皮擦 / 加轨行为 / 日志保留）。
    #[default]
    General,
    /// 快捷键页（当前只有标题与占位说明）。
    Shortcuts(ShortcutsPageModel),
    /// 洋葱皮页（渲染后端 GPU/CPU，加载 MIDI 后锁定）。
    OnionSkin(OnionSkinPageModel),
    /// 兼容性页（GPU 检查 / 启动开关 / Domino 剪贴板）。
    Compatibility(CompatibilityPageModel),
    /// 音频页（输出类型 / 设备 / 引擎专属参数）。
    Audio(AudioPageModel),
    /// 界面页（语言 / 主题卡片 / 字体 / 交互 / 自动滚动）。
    Ui(Box<super::UiPageModel>),
}

/// 快捷键页。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShortcutsPageModel {
    /// 标题。
    pub title: String,
    /// 占位说明。
    pub placeholder: String,
}

/// 洋葱皮页。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OnionSkinPageModel {
    /// 标题。
    pub title: String,
    /// 「洋葱皮渲染方式」标签。
    pub label: String,
    /// 下拉选项（`GPU` / `CPU`）。
    pub options: Vec<String>,
    /// 当前选项下标。
    pub selected: usize,
    /// 是否已锁定（加载 MIDI 后）。
    pub locked: bool,
    /// 锁定态下展示的当前后端文本。
    pub locked_value: String,
    /// 锁定提示。
    pub locked_hint: String,
    /// CPU 后端能力边界提示。
    pub hint: String,
}

/// 兼容性页。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompatibilityPageModel {
    /// 标题。
    pub title: String,
    /// 手动检查按钮文本（检测中会变成「检测中…」）。
    pub check_label: String,
    /// 是否可点检查（检测中 / 冷却 / 会话锁定期间禁用）。
    pub check_enabled: bool,
    /// 复制诊断按钮文本（复制后变成「已复制」）。
    pub copy_label: String,
    /// 是否可复制（只有已有结果时）。
    pub copy_enabled: bool,
    /// 结果与门控提示（按顺序显示，`dim` = 辅助文字）。
    pub result_lines: Vec<CompatLine>,
    /// 启动时检查开关。
    pub check_on_startup: bool,
    /// 开关标签。
    pub check_on_startup_label: String,
    /// 开关说明（悬浮提示）。
    pub check_on_startup_hint: String,
    /// 启动时显示警告（`!warning_suppressed`）。
    pub show_warning: bool,
    /// 开关标签。
    pub warning_label: String,
    /// 开关说明。
    pub warning_hint: String,
    /// Domino 剪贴板互通开关。
    pub domino: bool,
    /// 是否可操作（仅 Windows 实现）。
    pub domino_enabled: bool,
    /// 开关标签。
    pub domino_label: String,
    /// 开关说明。
    pub domino_hint: String,
}

/// 兼容性页的一行结果文本。
#[derive(Debug, Clone, PartialEq)]
pub struct CompatLine {
    /// 文本。
    pub text: String,
    /// 是否辅助文字（次级颜色）。
    pub dim: bool,
}
