//! lumino-message — 消息与共享类型定义
//!
//! 本 crate 定义了整个 lumino 应用的消息传递系统和跨模块共享类型。
//! Message 枚举是泛型的，由上层 crate（lumino-ui）实例化具体的 UI 事件类型。

pub mod audio_export;
pub mod batch_edit;
pub mod collaboration;
pub mod context_menu;
pub mod custom_precision;
pub mod load_confirm;
pub mod loop_range;
pub mod project_settings;
pub mod settings_dialog;
pub mod speed_change;
pub mod types;
pub mod velocity;
pub mod video_export;

pub use audio_export::AudioExportAction;
pub use batch_edit::{BatchEditAction, BatchEditField};
pub use collaboration::CollaborationAction;
pub use context_menu::{
    PianoRollContextMenuAction, PianoRollContextMenuItem, TrackContextMenuItem,
};
pub use custom_precision::CustomPrecisionAction;
pub use load_confirm::LoadConfirmAction;
pub use loop_range::LoopRangeAction;
pub use project_settings::ProjectSettingsAction;
pub use settings_dialog::SettingsDialogAction;
pub use speed_change::SpeedChangeAction;
pub use types::*;
pub use velocity::VelocityAction;
pub use video_export::VideoExportAction;

pub use lumino_core::{AudioAction, DotType, NotePrecision, Tool};

use lumino_event::Event;

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
        offset: Point2,
        size: Size2,
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
    /// 视频导出动作
    VideoExport(VideoExportAction),
    /// 批量消息（用于 canvas 等一次事件需要发布多条消息的场景）
    Batch(Vec<Message<W, S, Se, T>>),
    /// 钢琴卷帘右键上下文菜单动作
    PianoRollContextMenu(PianoRollContextMenuAction),
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
}

pub const fn null<W, S, Se, T>() -> Message<W, S, Se, T> {
    Message::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── PerfData ───

    #[test]
    fn test_perf_data_default() {
        let data = PerfData::default();
        assert_eq!(data.fps, 0.0);
        assert_eq!(data.cpu_usage, 0.0);
        assert_eq!(data.memory_mb, 0.0);
        assert_eq!(data.gpu_frame_time_ms, 0.0);
    }

    #[test]
    fn test_perf_data_new() {
        let data = PerfData::new(60.0, 25.5, 512.0, 16.7);
        assert_eq!(data.fps, 60.0);
        assert_eq!(data.cpu_usage, 25.5);
        assert_eq!(data.memory_mb, 512.0);
        assert_eq!(data.gpu_frame_time_ms, 16.7);
    }

    // ─── TupletType ───

    #[test]
    fn test_tuplet_type_default() {
        assert_eq!(TupletType::default(), TupletType::None);
    }

    #[test]
    fn test_tuplet_type_value() {
        assert_eq!(TupletType::Triplet.value(), 3);
        assert_eq!(TupletType::Quintuplet.value(), 5);
        assert_eq!(TupletType::Sextuplet.value(), 6);
        assert_eq!(TupletType::Septuplet.value(), 7);
        assert_eq!(TupletType::None.value(), 1);
    }

    #[test]
    fn test_tuplet_type_all() {
        let all = TupletType::all();
        assert_eq!(all.len(), 5);
    }

    // ─── SpeedFactor ───

    #[test]
    fn test_speed_factor_default() {
        assert_eq!(SpeedFactor::default(), SpeedFactor::X05);
    }

    #[test]
    fn test_speed_factor_value() {
        assert_eq!(SpeedFactor::X025.value(), 0.25);
        assert_eq!(SpeedFactor::X05.value(), 0.5);
        assert_eq!(SpeedFactor::X1.value(), 1.0);
        assert_eq!(SpeedFactor::X2.value(), 2.0);
        assert_eq!(SpeedFactor::X4.value(), 4.0);
    }

    #[test]
    fn test_speed_factor_display() {
        assert_eq!(SpeedFactor::X05.display_name(), "×0.5");
        assert_eq!(SpeedFactor::X2.display_name(), "×2.0");
    }

    // ─── AudioChannels ───

    #[test]
    fn test_audio_channels_default() {
        assert_eq!(AudioChannels::default(), AudioChannels::Stereo);
    }

    #[test]
    fn test_audio_channels_display() {
        assert_eq!(AudioChannels::Mono.to_string(), "单声道");
        assert_eq!(AudioChannels::Stereo.to_string(), "立体声");
    }

    #[test]
    fn test_audio_channels_channel_count() {
        assert_eq!(AudioChannels::Mono.channel_count(), 1);
        assert_eq!(AudioChannels::Stereo.channel_count(), 2);
    }

    // ─── AudioFormat ───

    #[test]
    fn test_audio_format_default() {
        assert_eq!(AudioFormat::default(), AudioFormat::WAV);
    }

    #[test]
    fn test_audio_format_display() {
        assert_eq!(AudioFormat::WAV.to_string(), "WAV");
        assert_eq!(AudioFormat::FLAC.to_string(), "FLAC");
        assert_eq!(AudioFormat::MP3.to_string(), "MP3");
        assert_eq!(AudioFormat::Ogg.to_string(), "Ogg Vorbis");
        assert_eq!(AudioFormat::WavPack.to_string(), "WavPack");
    }

    #[test]
    fn test_audio_format_extension() {
        assert_eq!(AudioFormat::WAV.extension(), "wav");
        assert_eq!(AudioFormat::FLAC.extension(), "flac");
        assert_eq!(AudioFormat::MP3.extension(), "mp3");
        assert_eq!(AudioFormat::Ogg.extension(), "ogg");
        assert_eq!(AudioFormat::WavPack.extension(), "wv");
    }

    #[test]
    fn test_audio_format_needs_ffmpeg() {
        assert!(!AudioFormat::WAV.needs_ffmpeg());
        assert!(!AudioFormat::FLAC.needs_ffmpeg());
        assert!(AudioFormat::MP3.needs_ffmpeg());
        assert!(AudioFormat::Ogg.needs_ffmpeg());
        assert!(AudioFormat::WavPack.needs_ffmpeg());
    }

    // ─── CcOption ───

    #[test]
    fn test_cc_option_display() {
        let bend = CcOption::Bend;
        assert!(bend.to_string().contains("Bend"));

        let cc7 = CcOption::Cc(7);
        assert!(cc7.to_string().contains("Volume"));
    }

    // ─── CC_CONTROLLER_NAMES ───

    #[test]
    fn test_cc_controller_names_known() {
        let names = CC_CONTROLLER_NAMES;
        assert!(names.contains(&(0, "Bank Select")));
        assert!(names.contains(&(7, "Volume")));
        assert!(names.contains(&(10, "Pan")));
        assert!(names.contains(&(64, "Sustain Pedal")));
        assert!(names.contains(&(127, "Poly Mode")));
    }

    #[test]
    fn test_cc_controller_names_all_128() {
        assert_eq!(CC_CONTROLLER_NAMES.len(), 128);
        let mut seen = std::collections::HashSet::new();
        for (num, _) in CC_CONTROLLER_NAMES {
            assert!(seen.insert(num), "Duplicate CC number: {}", num);
        }
    }

    // ─── EditorAction ───

    #[test]
    fn test_editor_action_clone() {
        let action = EditorAction::DeletePressed;
        let cloned = action.clone();
        assert!(matches!(cloned, EditorAction::DeletePressed));
    }

    #[test]
    fn test_editor_action_debug() {
        let action = EditorAction::Undo;
        let debug = format!("{:?}", action);
        assert!(debug.contains("Undo"));
    }

    // ─── Message null helper ───

    #[test]
    fn test_null_message() {
        let msg: Message<(), (), (), ()> = null();
        assert!(matches!(msg, Message::Null));
    }

    // ─── AudioExportAction ───

    #[test]
    fn test_audio_export_action_variants() {
        let action = AudioExportAction::OpenPanel;
        assert!(matches!(action, AudioExportAction::OpenPanel));

        let action = AudioExportAction::ClosePanel;
        assert!(matches!(action, AudioExportAction::ClosePanel));

        let action = AudioExportAction::BitrateChanged("320".to_string());
        assert!(matches!(action, AudioExportAction::BitrateChanged(_)));

        let action = AudioExportAction::IgnoreProgramChangesChanged(true);
        assert!(matches!(
            action,
            AudioExportAction::IgnoreProgramChangesChanged(_)
        ));

        let action = AudioExportAction::FilterVelocityChanged(true);
        assert!(matches!(
            action,
            AudioExportAction::FilterVelocityChanged(_)
        ));

        let action = AudioExportAction::FilterKeyChanged(true);
        assert!(matches!(action, AudioExportAction::FilterKeyChanged(_)));
    }

    // ─── CollaborationAction ───

    #[test]
    fn test_collaboration_action_variants() {
        let action = CollaborationAction::OpenDialog;
        assert!(matches!(action, CollaborationAction::OpenDialog));

        let action = CollaborationAction::Disconnect;
        assert!(matches!(action, CollaborationAction::Disconnect));

        let action = CollaborationAction::Connect {
            host: "localhost".to_string(),
            port: 3000,
            username: "test".to_string(),
            invite_code: None,
        };
        assert!(matches!(action, CollaborationAction::Connect { .. }));
    }

    // ─── LoopRangeAction ───

    #[test]
    fn test_loop_range_action_variants() {
        let action = LoopRangeAction::Toggle;
        assert!(matches!(action, LoopRangeAction::Toggle));

        let action = LoopRangeAction::SetRange(0.0, 100.0);
        assert!(matches!(action, LoopRangeAction::SetRange(_, _)));
    }

    // ─── SpeedChangeAction ───

    #[test]
    fn test_speed_change_action_variants() {
        let action = SpeedChangeAction::OpenDialog;
        assert!(matches!(action, SpeedChangeAction::OpenDialog));

        let action = SpeedChangeAction::FactorChanged("0.5".to_string());
        assert!(matches!(action, SpeedChangeAction::FactorChanged(_)));
    }

    // ─── VelocityAction ───

    #[test]
    fn test_velocity_action_variants() {
        let action = VelocityAction::DragStart(0, 100);
        assert!(matches!(action, VelocityAction::DragStart(_, _)));

        let action = VelocityAction::ToggleMode;
        assert!(matches!(action, VelocityAction::ToggleMode));

        let action = VelocityAction::TempoAdd(0.0, 120.0);
        assert!(matches!(action, VelocityAction::TempoAdd(_, _)));
    }

    // ─── ThreadingOption ───

    #[test]
    fn test_threading_option_display() {
        assert_eq!(ThreadingOption::None.to_string(), "关闭");
        assert_eq!(ThreadingOption::Auto.to_string(), "自动");
        assert_eq!(ThreadingOption::Manual(4).to_string(), "4 线程");
    }

    // ─── Interpolation ───

    #[test]
    fn test_interpolation_default() {
        assert_eq!(Interpolation::default(), Interpolation::Linear);
    }

    #[test]
    fn test_interpolation_display() {
        assert_eq!(Interpolation::None.to_string(), "无插值");
        assert_eq!(Interpolation::Linear.to_string(), "线性插值");
    }

    // ─── PianoRollContextMenu ───

    #[test]
    fn test_piano_roll_context_menu_message() {
        let msg: Message<(), (), (), ()> = Message::PianoRollContextMenu(
            PianoRollContextMenuAction::ItemClicked(PianoRollContextMenuItem::Copy),
        );
        assert!(matches!(
            msg,
            Message::PianoRollContextMenu(PianoRollContextMenuAction::ItemClicked(
                PianoRollContextMenuItem::Copy
            ))
        ));
    }
}
