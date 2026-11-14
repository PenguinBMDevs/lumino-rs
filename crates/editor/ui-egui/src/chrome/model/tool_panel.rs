//! 悬浮绘制工具条及其锚定面板的只读视图模型。
//!
//! 从 `chrome/model.rs` 拆出（REF-001：单文件 < 400 行）。工具条本体与
//! 五个面板（画刷 / 形状 / 分音符填充 / 文字参数 / 粗细）全部由生产状态
//! 镜像而来；点击只回传动作，`lumino-ui` 负责翻译成 `Message`。

use lumino_core::{TextLayoutMode, TextParams};

use crate::icons::Icon;

/// 悬浮绘制工具条（胶囊）+ 至多一个锚定面板。
#[derive(Debug, Clone)]
pub struct ToolPanelModel {
    /// 相对卷帘区中心的水平偏移与距底部的内缩（逻辑点）。
    pub offset: (f32, f32),
    /// 卷帘区左边界（左侧栏占用宽度，逻辑点）。
    pub left_inset: f32,
    /// 卷帘区上边界（标题栏 + 工具栏，逻辑点）。
    pub top_inset: f32,
    /// 「粗细」按钮的悬浮提示。
    pub thickness_tooltip: String,
    /// 工具条目（鼠标 / 曲线 / 颜料桶 / 画刷 / 形状 / 文字）。
    pub items: Vec<ToolPanelItemModel>,
    /// 「粗细」入口是否处于打开高亮态。
    pub thickness_selected: bool,
    /// 当前打开的锚定面板（五者互斥）。
    pub panel: Option<ToolPanelSettingsPanel>,
}

/// 悬浮条工具条目。
#[derive(Debug, Clone)]
pub struct ToolPanelItemModel {
    /// 条目种类（点击后原样回传）。
    pub kind: ToolPanelItemKind,
    /// 图标（形状条目随当前图形类型切换）。
    pub icon: Icon,
    /// 悬浮提示。
    pub tooltip: String,
    /// 是否处于选中态（工具激活 / 填充开启）。
    pub selected: bool,
    /// `true` = 已启用且有设置 → 点击弹设置面板；`false` = 普通选择。
    pub settings_request: bool,
}

/// 悬浮条工具条目种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPanelItemKind {
    /// 鼠标（选形）。
    Mouse,
    /// 曲线。
    Curve,
    /// 三点圆弧（DRAW-014）。
    Arc,
    /// 颜料桶（填充开关）。
    FillBucket,
    /// 画刷。
    Brush,
    /// 形状。
    Shape,
    /// 文字。
    Text,
}

/// 锚定在触发按钮上方的设置面板。
#[derive(Debug, Clone)]
pub enum ToolPanelSettingsPanel {
    /// 画刷（全局粗细只读 + 绘制行为入口）。
    Brush(BrushPanelModel),
    /// 形状（矩形 / 圆形 / 三角形 + 形状抽屉）。
    Shape(ShapePanelModel),
    /// 颜料桶「分音符填充」。
    FillDivision(FillDivisionPanelModel),
    /// 文字参数。
    TextParams(TextParamsPanelModel),
    /// 全局绘制粗细。
    Thickness(ThicknessPanelModel),
}

impl ToolPanelSettingsPanel {
    /// 面板归属的触发按钮（决定锚点）。
    pub fn owner(&self) -> ToolPanelPanelOwner {
        match self {
            Self::Brush(_) => ToolPanelPanelOwner::Item(ToolPanelItemKind::Brush),
            Self::Shape(_) => ToolPanelPanelOwner::Item(ToolPanelItemKind::Shape),
            Self::FillDivision(_) => ToolPanelPanelOwner::Item(ToolPanelItemKind::FillBucket),
            Self::TextParams(_) => ToolPanelPanelOwner::Item(ToolPanelItemKind::Text),
            Self::Thickness(_) => ToolPanelPanelOwner::Thickness,
        }
    }
}

/// 面板归属：工具条目或右端「粗细」按钮。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPanelPanelOwner {
    /// 工具条目。
    Item(ToolPanelItemKind),
    /// 右端「粗细」按钮。
    Thickness,
}

/// 画刷面板。
#[derive(Debug, Clone)]
pub struct BrushPanelModel {
    /// 当前全局粗细。
    pub thickness: u8,
    /// 粗细下限。
    pub min: u8,
    /// 粗细上限。
    pub max: u8,
    /// 标题（「粗细」）。
    pub title: String,
    /// 当前值文本（`20 层 / 100`）。
    pub value: String,
    /// 「绘制行为」按钮文本。
    pub behavior: String,
    /// 是否显示「绘制行为」入口（粗细 > 1）。
    pub show_behavior: bool,
}

/// 形状面板。
#[derive(Debug, Clone)]
pub struct ShapePanelModel {
    /// 当前图形类型。
    pub selected: ShapeKind,
    /// 三种图形的悬浮名称。
    pub rectangle: String,
    /// 圆形。
    pub circle: String,
    /// 三角形。
    pub triangle: String,
    /// 形状抽屉入口的悬浮名称。
    pub drawer: String,
}

/// 基本图形类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// 矩形。
    Rectangle,
    /// 圆形。
    Circle,
    /// 三角形。
    Triangle,
}

/// 颜料桶「分音符填充」面板。
#[derive(Debug, Clone)]
pub struct FillDivisionPanelModel {
    /// 输入框文本。
    pub value: String,
    /// 校验错误（有值时红字显示）。
    pub error: Option<String>,
    /// 前缀（「使用」）。
    pub prefix: String,
    /// 后缀（「分音符填充」）。
    pub suffix: String,
    /// 说明文本。
    pub hint: String,
    /// 「关闭填充」按钮。
    pub disable: String,
    /// 「取消」按钮。
    pub cancel: String,
    /// 「确定」按钮。
    pub ok: String,
}

/// 文字参数面板。
#[derive(Debug, Clone)]
pub struct TextParamsPanelModel {
    /// 当前参数（整体回传，避免面板持有第二份权威）。
    pub params: TextParams,
    /// 当前布局模式（决定 Mode 行显示与切换目标）。
    pub mode: TextLayoutMode,
    /// 可选字体（已排序去重；当前字体必然在列表内）。
    pub fonts: Vec<String>,
}

/// 「粗细」面板。
#[derive(Debug, Clone)]
pub struct ThicknessPanelModel {
    /// 当前值。
    pub value: u8,
    /// 下限。
    pub min: u8,
    /// 上限。
    pub max: u8,
    /// 标题（「粗细」）。
    pub title: String,
    /// 当前值文本（`20 层 (1 - 100)`）。
    pub value_text: String,
    /// 说明文本。
    pub hint: String,
}
