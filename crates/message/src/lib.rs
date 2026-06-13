//! lumino-message — 消息与共享类型定义
//!
//! 本 crate 定义了整个 lumino 应用的消息传递系统和跨模块共享类型。
//! Message 枚举是泛型的，由上层 crate（lumino-ui）实例化具体的 UI 事件类型。

pub mod audio_export;
pub mod collaboration;
pub mod loop_range;
pub mod pattern;
pub mod speed_change;
pub mod types;
pub mod velocity;

pub use audio_export::AudioExportAction;
pub use collaboration::CollaborationAction;
pub use loop_range::LoopRangeAction;
pub use pattern::PatternAction;
pub use speed_change::SpeedChangeAction;
pub use types::*;
pub use velocity::VelocityAction;

use lumino_event::Event;

/// 编辑器动作
#[derive(Debug, Clone)]
pub enum EditorAction {
    Pressed {
        pos: iced_core::Point,
        shift: bool,
    },
    Moved(iced_core::Point),
    Released,
    Scrolled {
        delta_x: f32,
        delta_y: f32,
    },
    /// 双击事件
    DoubleClicked(iced_core::Point),
    /// 删除键按下（Delete 或 Backspace）
    DeletePressed,
    /// 剪切
    Cut,
    /// 复制
    Copy,
    /// 粘贴
    Paste,
    /// 全选
    SelectAll,
    /// 撤销
    Undo,
    /// 重做
    Redo,
    /// 标尺 scrubbing：设置播放位置（tick 值）
    Scrubbed {
        tick: f32,
    },
    /// 演奏指示线拖拽开始（固定指示线模式下）
    IndicatorDragStart {
        x: f32,
    },
    /// 演奏指示线拖拽移动
    IndicatorDragMove {
        x: f32,
    },
}

/// 音频动作
#[derive(Debug, Clone)]
pub enum AudioAction {
    PlayNote { key: u8, velocity: u8 },
    StopNote { key: u8 },
}

/// 应用消息
///
/// 泛型参数：
/// - `W`: 窗口事件类型（由 lumino-ui 的 window::Event 实例化）
/// - `S`: 侧边栏事件类型（由 lumino-ui 的 sidebar::Event 实例化）
/// - `Se`: 设置事件类型（由 lumino-ui 的 settings::Event 实例化）
/// - `T`: 工具栏事件类型（由 lumino-ui 的 toolbar::Event 实例化）
#[derive(Debug, Clone)]
pub enum Message<W, S, Se, T> {
    Core(Event),
    Window(W),
    Sidebar(S),
    Progress(Option<(String, f64)>),
    ScrollbarScrolled(f32),
    ScrollbarScrolledY(f32),
    /// 工程走带水平滚动
    ArrangementScrollX(f32),
    /// 工程走带垂直滚动
    ArrangementScrollY(f32),
    /// 工程走带水平缩放
    ArrangementZoomX {
        zoom: f32,
        fixed_ratio: f32,
    },
    /// 工程走带垂直缩放
    ArrangementZoomY {
        zoom: f32,
        fixed_ratio: f32,
    },
    ZoomXChanged {
        zoom: f32,
        fixed_ratio: f32,
    },
    ZoomYChanged {
        zoom: f32,
        fixed_ratio: f32,
    },
    /// Canvas 位置和尺寸更新
    CanvasBoundsChanged {
        offset: iced_core::Point,
        size: iced_core::Size,
    },
    /// 菜单状态更新
    MenuStateChanged(bool),
    EditorAction(EditorAction),
    AudioAction(AudioAction),
    /// 设置面板事件
    Settings(Se),
    /// 切换设置面板显示状态
    ToggleSettings,
    /// 工具栏事件
    Toolbar(T),
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
    /// 协作动作
    Collaboration(CollaborationAction),
    /// 加载确认对话框 - 确认
    ConfirmLoadConfirm,
    /// 加载确认对话框 - 取消
    CloseLoadConfirmDialog,
    /// 打开工程设置对话框
    OpenProjectSettingsDialog,
    /// 关闭工程设置对话框
    CloseProjectSettingsDialog,
    /// 确认工程设置
    ConfirmProjectSettings,
    /// 工程设置 - 项目名称变更
    ProjectSettingsTitleChanged(String),
    /// 工程设置 - BPM 速度变更
    ProjectSettingsTempoChanged(String),
    /// 工程设置 - 版权信息变更
    ProjectSettingsCopyrightChanged(String),
    /// 打开设置对话框
    OpenSettingsDialog,
    /// 关闭设置对话框
    CloseSettingsDialog,
    /// 力度编辑面板动作
    Velocity(VelocityAction),
    /// 力度面板高度调整
    VelocityPanelResize(f32),
    /// 性能面板切换
    PerformancePanelToggled,
    /// 性能监控数据更新
    PerfUpdate(PerfData),
    /// 空消息标记
    Null,
    /// Ctrl 键状态变更
    CtrlKeyChanged(bool),
    ShiftKeyChanged(bool),
    /// 模式切换（编辑器/瀑布流）
    ModeToggled,
    /// 动画帧更新（用于弹簧物理模拟）
    AnimationTick,
    /// 循环区域事件
    LoopRange(LoopRangeAction),
    /// MIDI 输入事件（从 MIDI 设备收到的原始数据）
    MidiInputEvent {
        data: Vec<u8>,
    },
    /// 音频导出动作
    AudioExport(AudioExportAction),
    /// 音符变速动作
    SpeedChange(SpeedChangeAction),
    /// Pattern 编辑动作
    Pattern(PatternAction),
}

pub const fn null<W, S, Se, T>() -> Message<W, S, Se, T> {
    Message::Null
}
