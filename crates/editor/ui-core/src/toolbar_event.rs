//! Toolbar 事件子模块
//!
//! 包括工具栏事件枚举及其辅助类型。

use iced_core::Point;
use lumino_message::{DotType, NotePrecision, Tool, TupletType};

use crate::button_descs::ButtonId;

// 子模块（`Event` 的 `Message` 构造器）
mod constructors;

/// 工具栏事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 播放
    Play,
    /// 暂停
    Pause,
    /// 停止
    Stop,
    /// 跳到上个位置/开头
    SkipBackward,
    /// 跳到下个位置/末尾
    SkipForward,
    /// 撤销
    Undo,
    /// 重做
    Redo,
    /// 选择工具
    ToolSelected(Tool),
    /// 颜料桶填充模式开关（仅曲线工具激活时可用）
    FillToggled(bool),
    /// 量化音符
    Quantize,
    /// 精度设置变更
    PrecisionChanged(NotePrecision),
    /// 打开自定义精度对话框
    OpenCustomPrecisionDialog,
    /// 关闭自定义精度对话框
    CloseCustomPrecisionDialog,
    /// 确认自定义精度
    ConfirmCustomPrecision,
    /// 三连音数量变更
    CustomPrecisionTupletCountChanged(String),
    /// 三连音类型变更
    CustomPrecisionTupletTypeChanged(TupletType),
    /// 符点类型变更
    CustomPrecisionDotTypeChanged(DotType),
    /// 分音符值变更
    CustomPrecisionNoteValueChanged(String),
    /// 除数变更
    CustomPrecisionDivisorChanged(String),
    /// 打开协作对话框
    OpenCollaborationDialog,
    /// 打开工程设置对话框
    OpenProjectSettingsDialog,
    /// 打开内存监控对话框
    OpenMemoryMonitorDialog,
    /// 自动滚动模式切换
    AutoScrollModeChanged,
    /// 循环播放切换
    ToggleLoop,
    /// 开始拖拽调整高度
    ResizeDragStarted(Point),
    /// 拖拽中调整高度
    ResizeDragged(Point),
    /// 结束拖拽调整高度
    ResizeDragEnded,
    /// 录制
    Record,
    /// 停止录制
    RecordStop,
    /// 音符变速
    SpeedChange,
    /// 垂直翻转选中音符
    FlipVertical,
    /// 水平翻转选中音符
    FlipHorizontal(FlipHorizontalMode),
    /// 移调 +N 半音
    TransposeUp(i16),
    /// 移调 -N 半音
    TransposeDown(i16),
    /// 音符分割（Razor 工具 onclick，选中时分割选中音符）
    Split,
    /// 音符合并
    Glue,
    /// 音符连奏（同音连接）
    Tie,
    /// 切换溢出菜单显示/隐藏
    ToggleOverflowMenu,
    /// 关闭溢出菜单
    CloseOverflowMenu,
    /// Toggle PPQ 编辑模式（开始/取消）。u16 = 当前 PPQ 值
    PpqEditToggled(u16),
    /// PPQ 编辑输入变更
    PpqEditChanged(String),
    /// PPQ 编辑确认（按 Enter 或外部点击）
    PpqEditConfirmed,
    /// 鼠标悬停在工具栏按钮上。携带按钮角色标识（None 表示鼠标离开按钮）
    ///
    /// 该事件用于驱动底部状态栏左侧的"功能按钮描述"显示区：
    /// 悬停时显示 `按钮名 - {解释说明}`，离开时清空。
    ButtonHovered(Option<ButtonId>),
    /// 图片转 MIDI 占位按钮（功能开发中）
    ImageToMidiClicked,
    /// 切换「绘制工具选择面板」显示/隐藏（颜料桶右侧小三角触发）
    ToggleToolPanel,
    /// 关闭「绘制工具选择面板」
    CloseToolPanel,
    /// 选择「绘制工具选择面板」中的某个条目
    ToolPanelItemSelected(ToolPanelItem),
    /// 切换「画刷工具下拉」（ctrl+点击附属按钮触发）
    ToggleBrushDropdown,
    /// 关闭「画刷工具下拉」
    CloseBrushDropdown,
    /// 画刷粗细度变更（下拉 +/- 步进，1-20）
    BrushThicknessChanged(u8),
    /// 切换「形状工具下拉」（ctrl+点击形状工具触发）：矩形/圆形/三角形选择
    ToggleShapeDropdown,
    /// 关闭「形状工具下拉」
    CloseShapeDropdown,
    /// 形状工具当前图形类型变更（矩形/圆形/三角形）
    ShapeTypeSelected(ShapeType),
}

/// 形状工具当前绘制的图形类型
///
/// 由「形状工具下拉」（ctrl+点击工具栏形状工具弹出）切换，并由工具栏状态变量
/// `Toolbar::current_shape` 持久保存，供编辑器侧绘制时读取。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeType {
    /// 矩形
    #[default]
    Rectangle,
    /// 圆形
    Circle,
    /// 三角形
    Triangle,
}

/// 绘制工具选择面板中的条目
///
/// 点击后由 `Toolbar::update` 翻译为具体的工具选择/设置动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPanelItem {
    /// 描边设置
    StrokeSettings,
    /// 曲线工具
    Curve,
    /// 填充桶
    FillBucket,
    /// 画刷工具
    Brush,
    /// 形状工具
    Shape,
    /// 文字输入
    Text,
    /// 橡皮擦
    Eraser,
}

/// 水平翻转模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlipHorizontalMode {
    /// 沿左右中轴翻转
    Center,
    /// 沿最左侧边缘翻转
    Left,
    /// 沿右侧边缘翻转
    Right,
}
