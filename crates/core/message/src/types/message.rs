//! 应用消息枚举定义
//!
//! Message 枚举是整个应用的消息中枢。拆分在此文件以保持 lib.rs 精简。
//! 通过 types.rs → lib.rs 的 pub use 链重新导出到 crate 根路径。

use lumino_event::Event;

use crate::audio_export::AudioExportAction;
use crate::batch_edit::BatchEditAction;
use crate::cloud_action::CloudAction;
use crate::collaboration::CollaborationAction;
use crate::custom_precision::CustomPrecisionAction;
use crate::load_confirm::LoadConfirmAction;
use crate::loop_range::LoopRangeAction;
use crate::project_settings::ProjectSettingsAction;
use crate::recover_track::RecoverTrackAction;
use crate::right_sidebar::RightSidebarAction;
use crate::settings_dialog::SettingsDialogAction;
use crate::speed_change::SpeedChangeAction;
use crate::types::editor::EditorAction;
use crate::types::geometry::{Point2, Size2};
use crate::types::ui::PerfData;
use crate::velocity::VelocityAction;
use crate::video_export::VideoExportAction;

use crate::context_menu::PianoRollContextMenuAction;

/// 应用消息
///
/// 泛型参数：
/// - `W`: 窗口事件类型（由 lumino-ui 的 window::Event 实例化）
/// - `S`: 侧边栏事件类型（由 lumino-ui 的 sidebar::Event 实例化）
/// - `Se`: 设置事件类型（由 lumino-ui 的 settings::Event 实例化）
/// - `T`: 工具栏事件类型（由 lumino-ui 的 toolbar::Event 实例化）
// TODO(P3): Core(Event) variant 较大（≥328 bytes），建议改用 Box<Event>。
// 当前 const fn 构造函数直接构造 Core(Event)，改为 Box 会打破 const 约束。
// 留待 P3 统一评审消息枚举内存布局后再处理。
#[allow(clippy::large_enum_variant)]
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
        offset: Point2,
        size: Size2,
    },
    /// 菜单状态更新
    MenuStateChanged(bool),
    EditorAction(EditorAction),
    AudioAction(lumino_core::AudioAction),
    /// 设置面板事件
    Settings(Se),
    /// 切换设置面板显示状态
    ToggleSettings,
    /// 工具栏事件
    Toolbar(T),
    /// 自定义精度对话框动作
    CustomPrecision(CustomPrecisionAction),
    /// 协作动作
    Collaboration(CollaborationAction),
    /// 加载确认对话框动作
    LoadConfirm(LoadConfirmAction),
    /// 工程设置对话框动作
    ProjectSettings(ProjectSettingsAction),
    /// 设置对话框动作
    SettingsDialog(SettingsDialogAction),
    /// 力度编辑面板动作
    Velocity(VelocityAction),
    /// 力度面板高度调整
    VelocityPanelResize(f32),
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
    /// 批量编辑动作
    BatchEdit(BatchEditAction),
    /// 找回删除音轨对话框动作
    RecoverTrack(RecoverTrackAction),
    /// 视频导出动作
    VideoExport(VideoExportAction),
    /// 批量消息（用于 canvas 等一次事件需要发布多条消息的场景）
    Batch(Vec<Message<W, S, Se, T>>),
    /// 钢琴卷帘右键上下文菜单动作
    PianoRollContextMenu(PianoRollContextMenuAction),
    /// 右侧栏动作
    RightSidebar(RightSidebarAction),
    /// 云存储动作
    Cloud(CloudAction),
    /// 工程走带：设置演奏指示线位置
    ArrangementCursorSet(f64),
    /// 工程走带：选择矩形变更（tick_start, tick_end, track_lo, track_hi）
    ArrangementSelectionChanged(Option<(f64, f64, usize, usize)>),
    /// 工程走带：清空选择
    ArrangementSelectionCleared,
    /// 工程走带：移动选中的音符
    ArrangementMoveNotes {
        delta_ticks: i64,
        delta_tracks: i32,
    },
    /// 工程走带：擦除矩形范围内的音符
    ArrangementErase {
        tick_start: f64,
        tick_end: f64,
        track_lo: usize,
        track_hi: usize,
    },
    /// 工程走带：在指定 tick/track 处分割音符
    ArrangementRazor {
        tick: f64,
        track: usize,
    },
    /// 工程走带：在指定音轨 tick 处添加音符
    ArrangementAddNote {
        track: usize,
        tick: f64,
        duration: f64,
        key: u8,
        velocity: u8,
    },
    /// 工程走带：ghost 音符预览列表更新
    ArrangementGhostNotesUpdated(Vec<(f64, f64, usize)>),
    /// 工程走带：拖拽中的框选矩形（实时预览，由 GPU 渲染）
    ArrangementDragSelectionRect(Option<(f64, f64, usize, usize)>),
    /// 工程走带：复制选中音符到剪贴板
    ArrangementCopy,
    /// 工程走带：从剪贴板粘贴音符
    ArrangementPaste,
    /// 工程走带：剪切选中音符（复制 + 删除）
    ArrangementCut,
    /// 工程走带：删除选中音符
    ArrangementDeleteSelection,
}

pub const fn null<W, S, Se, T>() -> Message<W, S, Se, T> {
    Message::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Message null helper ───

    #[test]
    fn test_null_message() {
        let msg: Message<(), (), (), ()> = null();
        assert!(matches!(msg, Message::Null));
    }

    // ─── PianoRollContextMenu ───

    #[test]
    fn test_piano_roll_context_menu_message() {
        let msg: Message<(), (), (), ()> = Message::PianoRollContextMenu(
            crate::context_menu::PianoRollContextMenuAction::ItemClicked(
                crate::context_menu::PianoRollContextMenuItem::Copy,
            ),
        );
        assert!(matches!(
            msg,
            Message::PianoRollContextMenu(
                crate::context_menu::PianoRollContextMenuAction::ItemClicked(
                    crate::context_menu::PianoRollContextMenuItem::Copy
                )
            )
        ));
    }
}
