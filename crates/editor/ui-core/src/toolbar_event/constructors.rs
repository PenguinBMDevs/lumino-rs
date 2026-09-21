//! `Event` 的 `Message` 构造器 —— 统一包装为 `Message::Toolbar`
//!
//! 视图层只调用这些构造器，避免在 UI 代码中散落 `Message::Toolbar(...)` 包装。

use iced_core::Point;
use lumino_message::{DotType, NotePrecision, Tool, TupletType};

use super::{Event, FlipHorizontalMode, ShapeType, ToolPanelItem};
use crate::Message;
use crate::button_descs::ButtonId;

impl Event {
    /// 构造"播放"的工具栏消息
    pub const fn play() -> Message {
        Message::Toolbar(Self::Play)
    }

    /// 构造"暂停"的工具栏消息
    pub const fn pause() -> Message {
        Message::Toolbar(Self::Pause)
    }

    /// 构造"停止"的工具栏消息
    pub const fn stop() -> Message {
        Message::Toolbar(Self::Stop)
    }

    /// 构造"跳到上个位置"的工具栏消息
    pub const fn skip_backward() -> Message {
        Message::Toolbar(Self::SkipBackward)
    }

    /// 构造"跳到下个位置"的工具栏消息
    pub const fn skip_forward() -> Message {
        Message::Toolbar(Self::SkipForward)
    }

    /// 构造"撤销"的工具栏消息
    pub const fn undo() -> Message {
        Message::Toolbar(Self::Undo)
    }

    /// 构造"重做"的工具栏消息
    pub const fn redo() -> Message {
        Message::Toolbar(Self::Redo)
    }

    /// 构造"选择工具"的工具栏消息
    pub fn tool_selected(tool: Tool) -> Message {
        Message::Toolbar(Self::ToolSelected(tool))
    }

    /// 构造"颜料桶填充模式开关"的工具栏消息
    pub const fn fill_toggled(enabled: bool) -> Message {
        Message::Toolbar(Self::FillToggled(enabled))
    }

    /// 构造"量化音符"的工具栏消息
    pub const fn quantize() -> Message {
        Message::Toolbar(Self::Quantize)
    }

    /// 构造"精度设置变更"的工具栏消息
    pub const fn precision_changed(precision: NotePrecision) -> Message {
        Message::Toolbar(Self::PrecisionChanged(precision))
    }

    /// 构造"打开自定义精度对话框"的工具栏消息
    pub const fn open_custom_precision_dialog() -> Message {
        Message::Toolbar(Self::OpenCustomPrecisionDialog)
    }

    /// 构造"关闭自定义精度对话框"的工具栏消息
    pub const fn close_custom_precision_dialog() -> Message {
        Message::Toolbar(Self::CloseCustomPrecisionDialog)
    }

    /// 构造"确认自定义精度"的工具栏消息
    pub const fn confirm_custom_precision() -> Message {
        Message::Toolbar(Self::ConfirmCustomPrecision)
    }

    /// 构造"三连音数量变更"的工具栏消息
    pub fn custom_precision_tuplet_count_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionTupletCountChanged(value))
    }

    /// 构造"三连音类型变更"的工具栏消息
    pub fn custom_precision_tuplet_type_changed(value: TupletType) -> Message {
        Message::Toolbar(Self::CustomPrecisionTupletTypeChanged(value))
    }

    /// 构造"符点类型变更"的工具栏消息
    pub fn custom_precision_dot_type_changed(value: DotType) -> Message {
        Message::Toolbar(Self::CustomPrecisionDotTypeChanged(value))
    }

    /// 构造"基础音符时值变更"的工具栏消息
    pub fn custom_precision_note_value_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionNoteValueChanged(value))
    }

    /// 构造"时值除数变更"的工具栏消息
    pub fn custom_precision_divisor_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionDivisorChanged(value))
    }

    /// 构造"打开协作对话框"的工具栏消息
    pub const fn open_collaboration_dialog() -> Message {
        Message::Toolbar(Self::OpenCollaborationDialog)
    }

    /// 构造"打开工程设置对话框"的工具栏消息
    pub const fn open_project_settings_dialog() -> Message {
        Message::Toolbar(Self::OpenProjectSettingsDialog)
    }

    /// 构造"打开内存监控对话框"的工具栏消息
    pub const fn open_memory_monitor_dialog() -> Message {
        Message::Toolbar(Self::OpenMemoryMonitorDialog)
    }

    /// 构造"自动滚动模式切换"的工具栏消息
    pub const fn auto_scroll_mode_changed() -> Message {
        Message::Toolbar(Self::AutoScrollModeChanged)
    }

    /// 构造"循环播放切换"的工具栏消息
    pub const fn toggle_loop() -> Message {
        Message::Toolbar(Self::ToggleLoop)
    }

    /// 构造"开始拖拽调整高度"的工具栏消息
    pub fn resize_drag_started() -> Message {
        Message::Toolbar(Self::ResizeDragStarted(Point::new(0.0, 0.0)))
    }

    /// 构造"拖拽中调整高度"的工具栏消息
    pub fn resize_dragged() -> Message {
        Message::Toolbar(Self::ResizeDragged(Point::new(0.0, 0.0)))
    }

    /// 构造"结束拖拽调整高度"的工具栏消息
    pub const fn resize_drag_ended() -> Message {
        Message::Toolbar(Self::ResizeDragEnded)
    }

    /// 构造"录制"的工具栏消息
    pub const fn record() -> Message {
        Message::Toolbar(Self::Record)
    }

    /// 构造"停止录制"的工具栏消息
    pub const fn record_stop() -> Message {
        Message::Toolbar(Self::RecordStop)
    }

    /// 构造"音符变速"的工具栏消息
    pub const fn speed_change() -> Message {
        Message::Toolbar(Self::SpeedChange)
    }

    /// 构造"垂直翻转选中音符"的工具栏消息
    pub const fn flip_vertical() -> Message {
        Message::Toolbar(Self::FlipVertical)
    }

    /// 构造"水平翻转选中音符"的工具栏消息
    pub fn flip_horizontal(mode: FlipHorizontalMode) -> Message {
        Message::Toolbar(Self::FlipHorizontal(mode))
    }

    /// 构造"向上移调"的工具栏消息
    pub const fn transpose_up(semitones: i16) -> Message {
        Message::Toolbar(Self::TransposeUp(semitones))
    }

    /// 构造"向下移调"的工具栏消息
    pub const fn transpose_down(semitones: i16) -> Message {
        Message::Toolbar(Self::TransposeDown(semitones))
    }

    /// 构造"音符分割"的工具栏消息
    pub const fn split() -> Message {
        Message::Toolbar(Self::Split)
    }

    /// 构造"音符合并"的工具栏消息
    pub const fn glue() -> Message {
        Message::Toolbar(Self::Glue)
    }

    /// 构造"音符连奏"的工具栏消息
    pub const fn tie() -> Message {
        Message::Toolbar(Self::Tie)
    }

    /// 构造"切换溢出菜单显示/隐藏"的工具栏消息
    pub const fn toggle_overflow_menu() -> Message {
        Message::Toolbar(Self::ToggleOverflowMenu)
    }

    /// 构造"关闭溢出菜单"的工具栏消息
    pub const fn close_overflow_menu() -> Message {
        Message::Toolbar(Self::CloseOverflowMenu)
    }

    /// 切换 PPQ 编辑模式。点击 PPQ 文字时携带当前值以初始化缓冲区。
    pub fn ppq_edit_toggled(ppq: u16) -> Message {
        Message::Toolbar(Self::PpqEditToggled(ppq))
    }

    /// PPQ 编辑输入变更（字符串必须仅含数字）。
    pub fn ppq_edit_changed(value: String) -> Message {
        Message::Toolbar(Self::PpqEditChanged(value))
    }

    /// PPQ 编辑确认（Enter 键或点击外部区域）。
    pub const fn ppq_edit_confirmed() -> Message {
        Message::Toolbar(Self::PpqEditConfirmed)
    }

    /// 鼠标悬停工具栏按钮。`id` 为按钮角色标识，传 `None` 表示离开按钮。
    pub fn button_hovered(id: Option<ButtonId>) -> Message {
        Message::Toolbar(Self::ButtonHovered(id))
    }

    /// 图片转 MIDI 占位按钮点击事件
    pub const fn image_to_midi_clicked() -> Message {
        Message::Toolbar(Self::ImageToMidiClicked)
    }

    /// 构造“切换绘制工具选择面板”的工具栏消息
    pub const fn toggle_tool_panel() -> Message {
        Message::Toolbar(Self::ToggleToolPanel)
    }

    /// 构造“关闭绘制工具选择面板”的工具栏消息
    pub const fn close_tool_panel() -> Message {
        Message::Toolbar(Self::CloseToolPanel)
    }

    /// 构造“选择绘制工具面板条目”的工具栏消息
    pub const fn tool_panel_item_selected(item: ToolPanelItem) -> Message {
        Message::Toolbar(Self::ToolPanelItemSelected(item))
    }

    /// 构造“切换画刷工具下拉”的工具栏消息
    pub const fn toggle_brush_dropdown() -> Message {
        Message::Toolbar(Self::ToggleBrushDropdown)
    }

    /// 构造“关闭画刷工具下拉”的工具栏消息
    pub const fn close_brush_dropdown() -> Message {
        Message::Toolbar(Self::CloseBrushDropdown)
    }

    /// 构造“画刷粗细度变更”的工具栏消息
    pub const fn brush_thickness_changed(thickness: u8) -> Message {
        Message::Toolbar(Self::BrushThicknessChanged(thickness))
    }

    /// 构造“切换形状工具下拉”的工具栏消息
    pub const fn toggle_shape_dropdown() -> Message {
        Message::Toolbar(Self::ToggleShapeDropdown)
    }

    /// 构造“关闭形状工具下拉”的工具栏消息
    pub const fn close_shape_dropdown() -> Message {
        Message::Toolbar(Self::CloseShapeDropdown)
    }

    /// 构造“形状类型变更”的工具栏消息
    pub const fn shape_type_selected(shape: ShapeType) -> Message {
        Message::Toolbar(Self::ShapeTypeSelected(shape))
    }
}
