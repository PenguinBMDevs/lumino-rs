//! 设置面板事件处理器
//!
//! 处理 Settings 消息，将设置变更同步到 Root 状态。
//! 设置事件双层结构：外层 Message::Settings(event)，内层 event 枚举各变体。

use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;
use lumino_core::storage::config::SynthBackend;
use lumino_ui_core::settings_event::OutputType;

/// 设置消息处理器
#[derive(Default)]
pub struct SettingsHandler;

impl SettingsHandler {
    /// 创建一个设置消息处理器
    pub fn new() -> Self {
        Self
    }
}

impl MessageHandler for SettingsHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        let Message::Settings(event) = msg else {
            return Some(msg);
        };

        root.settings.update(event.clone());

        match event {
            crate::settings::Event::EraserBehaviorChanged(behavior) => {
                root.editor.set_eraser_behavior(behavior);
            }
            crate::settings::Event::SelectionBoxModeChanged(mode) => {
                root.editor.set_selection_box_mode(mode);
                tracing::debug!("Root: 框选框模式切换为 {:?}", mode);
            }
            crate::settings::Event::VelocityFilterThresholdChanged(value) => {
                if let Ok(val) = value.parse::<u8>() {
                    root.visual.velocity_filter_threshold = val;
                    tracing::debug!("Root: 力度过滤阈值同步为 {}", val);
                    // 立即传播到播放引擎，让力度过滤实时生效。
                    root.update_playback_notes();
                }
            }
            crate::settings::Event::AutoScrollFixedPositionChanged(value) => {
                if let Ok(val) = value.parse::<u32>() {
                    let mut config = *root.editor.auto_scroll_config();
                    config.fixed_indicator_position = val;
                    root.editor.set_auto_scroll_config(config);
                    tracing::debug!("Root: 自动滚动固定位置同步为 {}", val);
                }
            }
            crate::settings::Event::AutoScrollPageTriggerOffsetChanged(value) => {
                if let Ok(val) = value.parse::<u32>() {
                    let mut config = *root.editor.auto_scroll_config();
                    config.page_trigger_offset = val;
                    root.editor.set_auto_scroll_config(config);
                    tracing::debug!("Root: 自动滚动翻页触发偏移同步为 {}", val);
                }
            }
            crate::settings::Event::AutoScrollPageReturnPositionChanged(value) => {
                if let Ok(val) = value.parse::<u32>() {
                    let mut config = *root.editor.auto_scroll_config();
                    config.page_return_position = val;
                    root.editor.set_auto_scroll_config(config);
                    tracing::debug!("Root: 自动滚动翻页返回位置同步为 {}", val);
                }
            }
            crate::settings::Event::IconHiDPIChanged(enabled) => {
                crate::resources::icon::set_hidpi_enabled(enabled);
                tracing::debug!("Root: HiDPI 图标渲染切换为 {}", enabled);
            }
            crate::settings::Event::Enable256keyChanged(enabled) => {
                let new_count: u16 = if enabled { 256 } else { 128 };
                root.editor.set_visible_key_count(new_count);
                root.editor.editor_state.view.key_count = new_count;
                tracing::debug!(
                    "Root: 256键模式切换为 {}，琴键数调整为 {}",
                    enabled,
                    new_count
                );
            }
            crate::settings::Event::LanguageChanged(lang) => {
                tracing::debug!("Root: 界面语言切换为 {:?}", lang);
            }
            crate::settings::Event::AutomationLineThicknessChanged(v) => {
                root.editor.velocity_panel.automation_line_thickness = v;
                tracing::debug!("Root: 自动化曲线连线粗细设置为 {}", v);
            }
            crate::settings::Event::TempoMaxBpmChanged(v) => {
                root.editor.velocity_panel.tempo_max_bpm = v;
                tracing::debug!("Root: Tempo BPM 上限设置为 {}", v);
            }
            crate::settings::Event::MonitorRefreshIntervalChanged(v) => {
                tracing::debug!("Root: 监控数据刷新间隔设置为 {}ms", v);
            }
            crate::settings::Event::ScanWinmmOutputs => {
                // 系统播表自动扫描（WinMM 输出设备列表）
                root.scan_winmm_outputs();
            }
            crate::settings::Event::WinmmOutputSelected(_id) => {
                tracing::debug!("Root: 已选择 WinMM 输出设备(播表)");
            }
            crate::settings::Event::ScanAudioOutputs => {
                // 音频播放输出设备自动扫描（CPAL 音频设备列表）
                root.scan_audio_outputs();
            }
            crate::settings::Event::AudioOutputSelected(_name) => {
                tracing::debug!("Root: 已选择音频播放输出设备");
            }
            crate::settings::Event::OutputTypeChanged(OutputType::System)
            | crate::settings::Event::SynthBackendChanged(SynthBackend::System) => {
                // 进入 WinMM 模式时自动扫描播表
                root.scan_winmm_outputs();
            }
            crate::settings::Event::OutputTypeChanged(OutputType::Builtin)
            | crate::settings::Event::SynthBackendChanged(SynthBackend::XSynth)
            | crate::settings::Event::SynthBackendChanged(SynthBackend::Lgs) => {
                // 进入内置软件合成器时自动扫描音频播放输出设备
                root.scan_audio_outputs();
            }
            _ => {} // 其他设置变更由 settings.update() 同步
        }

        None // 已处理
    }
}
