//! Toolbar 事件子模块

use iced_core::Point;

use crate::Message;
use crate::toolbar::{DotType, NotePrecision, Tool, TupletType};

/// 工具栏事件
#[derive(Debug, Clone)]
pub enum Event {
    Play,
    Pause,
    Stop,
    SkipBackward,
    SkipForward,
    Undo,
    Redo,
    ToolSelected(Tool),
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
    /// 移调 +1 半音
    TransposeUp,
    /// 移调 -1 半音
    TransposeDown,
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

impl Event {
    pub const fn play() -> Message {
        Message::Toolbar(Self::Play)
    }

    pub const fn pause() -> Message {
        Message::Toolbar(Self::Pause)
    }

    pub const fn stop() -> Message {
        Message::Toolbar(Self::Stop)
    }

    pub const fn skip_backward() -> Message {
        Message::Toolbar(Self::SkipBackward)
    }

    pub const fn skip_forward() -> Message {
        Message::Toolbar(Self::SkipForward)
    }

    pub const fn undo() -> Message {
        Message::Toolbar(Self::Undo)
    }

    pub const fn redo() -> Message {
        Message::Toolbar(Self::Redo)
    }

    pub fn tool_selected(tool: Tool) -> Message {
        Message::Toolbar(Self::ToolSelected(tool))
    }

    pub const fn quantize() -> Message {
        Message::Toolbar(Self::Quantize)
    }

    pub const fn precision_changed(precision: NotePrecision) -> Message {
        Message::Toolbar(Self::PrecisionChanged(precision))
    }

    pub const fn open_custom_precision_dialog() -> Message {
        Message::Toolbar(Self::OpenCustomPrecisionDialog)
    }

    pub const fn close_custom_precision_dialog() -> Message {
        Message::Toolbar(Self::CloseCustomPrecisionDialog)
    }

    pub const fn confirm_custom_precision() -> Message {
        Message::Toolbar(Self::ConfirmCustomPrecision)
    }

    pub fn custom_precision_tuplet_count_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionTupletCountChanged(value))
    }

    pub fn custom_precision_tuplet_type_changed(value: TupletType) -> Message {
        Message::Toolbar(Self::CustomPrecisionTupletTypeChanged(value))
    }

    pub fn custom_precision_dot_type_changed(value: DotType) -> Message {
        Message::Toolbar(Self::CustomPrecisionDotTypeChanged(value))
    }

    pub fn custom_precision_note_value_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionNoteValueChanged(value))
    }

    pub fn custom_precision_divisor_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionDivisorChanged(value))
    }

    pub const fn open_collaboration_dialog() -> Message {
        Message::Toolbar(Self::OpenCollaborationDialog)
    }

    pub const fn open_project_settings_dialog() -> Message {
        Message::Toolbar(Self::OpenProjectSettingsDialog)
    }

    pub const fn auto_scroll_mode_changed() -> Message {
        Message::Toolbar(Self::AutoScrollModeChanged)
    }

    pub const fn toggle_loop() -> Message {
        Message::Toolbar(Self::ToggleLoop)
    }

    pub fn resize_drag_started() -> Message {
        Message::Toolbar(Self::ResizeDragStarted(Point::new(0.0, 0.0)))
    }

    pub fn resize_dragged() -> Message {
        Message::Toolbar(Self::ResizeDragged(Point::new(0.0, 0.0)))
    }

    pub const fn resize_drag_ended() -> Message {
        Message::Toolbar(Self::ResizeDragEnded)
    }

    pub const fn record() -> Message {
        Message::Toolbar(Self::Record)
    }

    pub const fn record_stop() -> Message {
        Message::Toolbar(Self::RecordStop)
    }

    pub const fn speed_change() -> Message {
        Message::Toolbar(Self::SpeedChange)
    }

    pub const fn flip_vertical() -> Message {
        Message::Toolbar(Self::FlipVertical)
    }

    pub fn flip_horizontal(mode: FlipHorizontalMode) -> Message {
        Message::Toolbar(Self::FlipHorizontal(mode))
    }

    pub const fn transpose_up() -> Message {
        Message::Toolbar(Self::TransposeUp)
    }

    pub const fn transpose_down() -> Message {
        Message::Toolbar(Self::TransposeDown)
    }
}
