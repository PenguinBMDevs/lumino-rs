//! 设置面板状态模型与事件处理
//!
//! 内含 `Event` 分发逻辑（`SettingsPanel::update`），从 `lib.rs` 拆分而来。
//! - 构造器 `SettingsPanel::new` 见 `panel/new`（纯结构拆分，语义不变）；
//! - 阻塞式文件选择器（音色库 / 程序字体）见 `panel/file_pickers`；
//! - `update` 的分组事件处理见 `panel/update_groups`（按 UI 区块拆分，臂序与语义不变）。

use crate::{Event, SettingsPanel};

mod file_pickers;
mod new;
mod update_groups;

impl SettingsPanel {
    /// 处理来自 UI 的设置变更事件并更新面板状态。
    ///
    /// 根据事件类型分发到对应的设置子结构，数值类变化通过字符串解析
    /// 并自动忽略非法输入。
    ///
    /// # 参数
    /// * `event` — 需要处理的设置变更事件
    pub fn update(&mut self, event: Event) {
        // 按原 `match` 的臂序分组分派：每个事件只命中一个分组，组内臂序与拆分前一致。
        // 对 `&event` 做全变体编译期穷尽匹配、不引入 `_ => {}`：新增变体会在此报错。
        match &event {
            Event::MenuSelected(_)
            | Event::SynthBackendChanged(_)
            | Event::OutputTypeChanged(_)
            | Event::AudioEngineChanged(_)
            | Event::SoundfontPathChanged(_)
            | Event::BrowseSoundfont
            | Event::NativeTitlebarChanged(_)
            | Event::XSynthBufferChanged(_)
            | Event::XSynthSampleRateChanged(_)
            | Event::XSynthMaxVoicesChanged(_)
            | Event::XSynthMaxVoicesCustomInput(_) => self.update_audio_backend(event),

            Event::XSynthGlobalVoiceLimitChanged(_)
            | Event::XSynthVoiceTargetRatioChanged(_)
            | Event::XSynthSoftNpsGateChanged(_)
            | Event::LgsBlockSizeChanged(_)
            | Event::LgsOfflineBlockSizeChanged(_)
            | Event::LgsMaxVoicesChanged(_)
            | Event::LgsGlobalVoiceLimitChanged(_)
            | Event::LgsSoftNpsGateChanged(_)
            | Event::LgsVelocityFilterChanged(_)
            | Event::GpuBufferStatusEnabledChanged(_)
            | Event::GpuGovernorMetricsEnabledChanged(_)
            | Event::GpuLoadDisplayChanged(_) => self.update_voice_performance(event),

            Event::ThemeChanged(_)
            | Event::EraserBehaviorChanged(_)
            | Event::SelectionBoxModeChanged(_)
            | Event::ProgramFontNameChanged(_)
            | Event::ProgramFontPathChanged(_)
            | Event::BrowseProgramFont => self.update_editing_fonts(event),

            Event::AutoScrollFixedPositionChanged(_)
            | Event::AutoScrollPageTriggerOffsetChanged(_)
            | Event::AutoScrollPageReturnPositionChanged(_)
            | Event::VelocityFilterThresholdChanged(_)
            | Event::IconHiDPIChanged(_)
            | Event::Enable256keyChanged(_)
            | Event::NoteCountDisplayChanged(_)
            | Event::VelocityCurveStyleChanged(_) => self.update_view_settings(event),

            Event::DeviceSelected(_)
            | Event::ScanMidiDevices
            | Event::WinmmOutputSelected(_)
            | Event::ScanWinmmOutputs
            | Event::AudioOutputSelected(_)
            | Event::ScanAudioOutputs => self.update_midi_devices(event),

            Event::LanguagePreferenceChanged(_)
            | Event::OnionSkinBackendChanged(_)
            | Event::PlaybackKeyColorsEnabledChanged(_)
            | Event::TrackAddBehaviorChanged(_)
            | Event::PaletteChanged(_) => self.update_appearance(event),

            Event::HistoryTotalLimitChanged(_)
            | Event::HistoryEntryLimitChanged(_)
            | Event::MergeWindowMsChanged(_)
            | Event::InterceptNotificationChanged(_)
            | Event::AutosaveEnabledChanged(_)
            | Event::AutosaveIntervalMinsChanged(_)
            | Event::AutomationLineThicknessChanged(_)
            | Event::TempoMaxBpmChanged(_)
            | Event::TempoMaxBpmCustomOpen
            | Event::LogRetentionCountChanged(_)
            | Event::MonitorRefreshIntervalChanged(_)
            | Event::FirstScreenOpacityChanged(_) => self.update_edit_history(event),

            Event::GpuCheckOnStartupChanged(_)
            | Event::GpuWarningSuppressedChanged(_)
            | Event::DominoClipboardEnabledChanged(_)
            | Event::RunGpuCompatibilityCheck
            | Event::CopyGpuDiagnostics => self.update_compatibility(event),

            Event::AboutLogoClicked | Event::EchoCaveClicked => self.update_about(event),
        }
    }
}
