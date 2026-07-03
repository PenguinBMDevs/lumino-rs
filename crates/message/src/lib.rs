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
    PlayNote {
        key: u8,
        velocity: u8,
    },
    StopNote {
        key: u8,
    },
    /// 启动播放（同步 PlaybackManager 状态到 AudioEngine）
    StartPlayback,
    /// 暂停播放
    PausePlayback,
    /// 停止播放
    StopPlayback,
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

    // ─── NotePrecision ───

    #[test]
    fn test_note_precision_default() {
        assert_eq!(NotePrecision::default(), NotePrecision::Quarter);
    }

    #[test]
    fn test_note_precision_display() {
        assert_eq!(NotePrecision::Whole.to_string(), "全音符");
        assert_eq!(NotePrecision::Quarter.to_string(), "四分音符");
        assert_eq!(NotePrecision::Custom.to_string(), "自定义");
    }

    #[test]
    fn test_note_precision_as_ticks() {
        let ppq = 480;
        assert_eq!(NotePrecision::Whole.as_ticks(ppq), 480.0 * 4.0);
        assert_eq!(NotePrecision::Quarter.as_ticks(ppq), 480.0);
        assert_eq!(NotePrecision::Eighth.as_ticks(ppq), 480.0 / 2.0);
        assert_eq!(NotePrecision::OneTwentyEighth.as_ticks(ppq), 480.0 / 32.0);
    }

    #[test]
    fn test_note_precision_presets() {
        let presets = NotePrecision::presets();
        assert_eq!(presets.len(), 8);
        assert!(!presets.contains(&NotePrecision::Custom));
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

    // ─── DotType ───

    #[test]
    fn test_dot_type_default() {
        assert_eq!(DotType::default(), DotType::None);
    }

    #[test]
    fn test_dot_type_multiplier() {
        assert_eq!(DotType::None.multiplier(), 1.0);
        assert_eq!(DotType::Single.multiplier(), 1.5);
        assert_eq!(DotType::Double.multiplier(), 1.75);
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

    // ─── Tool ───

    #[test]
    fn test_tool_default() {
        assert_eq!(Tool::default(), Tool::Pointer);
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

    // ─── AudioFormat ───

    #[test]
    fn test_audio_format_default() {
        assert_eq!(AudioFormat::default(), AudioFormat::WAV);
    }

    #[test]
    fn test_audio_format_display() {
        assert_eq!(AudioFormat::WAV.to_string(), "WAV");
        assert_eq!(AudioFormat::FLAC.to_string(), "FLAC");
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

    // ─── AudioAction ───

    #[test]
    fn test_audio_action_play_note() {
        let action = AudioAction::PlayNote {
            key: 60,
            velocity: 100,
        };
        assert!(matches!(action, AudioAction::PlayNote { key: 60, .. }));
    }

    #[test]
    fn test_audio_action_stop_note() {
        let action = AudioAction::StopNote { key: 60 };
        assert!(matches!(action, AudioAction::StopNote { key: 60 }));
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
        let action = AudioExportAction::OpenDialog;
        assert!(matches!(action, AudioExportAction::OpenDialog));

        let action = AudioExportAction::Completed;
        assert!(matches!(action, AudioExportAction::Completed));

        let action = AudioExportAction::Failed("error".to_string());
        assert!(matches!(action, AudioExportAction::Failed(_)));
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

    // ─── PatternAction ───

    #[test]
    fn test_pattern_action_variants() {
        let action = PatternAction::Selected(1);
        assert!(matches!(action, PatternAction::Selected(1)));

        let action = PatternAction::DragEnd;
        assert!(matches!(action, PatternAction::DragEnd));
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
}
