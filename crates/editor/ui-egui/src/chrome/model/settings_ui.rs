//! 设置「界面」页的只读模型。
//!
//! 页面由通用行描述：语言 / 主题卡片 / 字体 / 若干开关 / 下拉 / 滑杆 / 输入框，
//! 以及两个小节标题。主题卡片的预览配色由生产侧按主题调色板预计算，
//! `lumino-ui-egui` 只负责绘制与点击回传。

use egui::Color32;

/// 界面页。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiPageModel {
    /// 页面标题。
    pub title: String,
    /// 页面行。
    pub rows: Vec<UiRow>,
}

/// 界面页的一行。
#[derive(Debug, Clone, PartialEq)]
pub enum UiRow {
    /// 小节标题（16px）。
    Section {
        /// 标题文本。
        title: String,
        /// 悬浮说明（空 = 无）。
        hint: String,
    },
    /// 下拉框。
    Combo {
        /// 字段。
        field: UiField,
        /// 标签。
        label: String,
        /// 悬浮说明。
        hint: String,
        /// 选项文本。
        options: Vec<String>,
        /// 选中下标。
        selected: usize,
        /// 控件宽度。
        width: f32,
    },
    /// 文本输入框（可带后缀说明）。
    Text {
        /// 字段。
        field: UiField,
        /// 标签。
        label: String,
        /// 悬浮说明。
        hint: String,
        /// 当前文本。
        value: String,
        /// 占位符。
        placeholder: String,
        /// 输入框宽度。
        width: f32,
        /// 后缀说明（如「像素」/「从左」）。
        suffix: String,
    },
    /// 开关（标签在右侧）。
    Checkbox {
        /// 字段。
        field: UiField,
        /// 标签。
        label: String,
        /// 悬浮说明。
        hint: String,
        /// 是否勾选。
        checked: bool,
    },
    /// 滑杆（右侧显示数值文本）。
    Slider {
        /// 字段。
        field: UiField,
        /// 标签。
        label: String,
        /// 悬浮说明。
        hint: String,
        /// 当前值。
        value: f32,
        /// 下限。
        min: f32,
        /// 上限。
        max: f32,
        /// 步进。
        step: f64,
        /// 数值文本（如 `1.5 px`）。
        value_text: String,
    },
    /// 按钮。
    Button {
        /// 字段。
        field: UiField,
        /// 按钮文本。
        label: String,
    },
    /// 只读辅助说明。
    Static {
        /// 文本。
        text: String,
    },
    /// 主题卡片网格。
    ThemeCards {
        /// 小节标签（「主题」）。
        label: String,
        /// 卡片。
        cards: Vec<ThemeCardModel>,
    },
}

/// 界面页可交互字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiField {
    /// 界面语言偏好（跟随系统 / 具体语言）。
    Language,
    /// 主题切换（值为规范主题名）。
    Theme,
    /// 系统字体名。
    ProgramFontName,
    /// 自定义字体路径。
    ProgramFontPath,
    /// 浏览字体文件。
    BrowseProgramFont,
    /// 使用系统原生标题栏。
    NativeTitlebar,
    /// HiDPI 图标渲染。
    IconHidpi,
    /// 音符总量显示位置。
    NoteCountDisplay,
    /// 自动滚动指示线固定位置。
    AutoScrollFixed,
    /// 翻页触发位置。
    AutoScrollTrigger,
    /// 翻页后回到位置。
    AutoScrollReturn,
    /// 框选框模式。
    SelectionBoxMode,
    /// 256 键钢琴卷帘。
    Enable256Key,
    /// 力度面板曲线样式。
    VelocityCurveStyle,
    /// 播放时键盘颜色指示。
    PlaybackKeyColors,
    /// 自动化曲线连线粗细。
    AutomationLineThickness,
    /// 底边栏监控刷新间隔。
    MonitorRefreshInterval,
}

/// 单张主题卡片。
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeCardModel {
    /// 展示名（高对比度为本地化名）。
    pub display: String,
    /// 规范主题名（点击回传）。
    pub value: String,
    /// 是否为当前主题。
    pub selected: bool,
    /// 卡片底色（主题背景）。
    pub bg: Color32,
    /// 标题色（主题主文字）。
    pub title: Color32,
    /// 迷你面板底色。
    pub control: Color32,
    /// 主文字条。
    pub bar_primary: Color32,
    /// 次级文字条。
    pub bar_secondary: Color32,
    /// 强调色条。
    pub accent: Color32,
    /// 分隔线。
    pub line: Color32,
    /// 次级分隔线（低透明度）。
    pub line_soft: Color32,
}
