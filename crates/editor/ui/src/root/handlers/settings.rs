//! 设置面板事件处理器
//!
//! 处理 Settings 消息，将设置变更同步到 Root 状态。
//! 设置事件双层结构：外层 Message::Settings(event)，内层 event 枚举各变体。

use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;
use lumino_core::storage::config::SynthBackend;
use lumino_ui_core::settings_event::OutputType;
use lumino_ui_core::state::GpuCheckUiState;
use std::time::Instant;

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
            crate::settings::Event::RunGpuCompatibilityCheck => {
                tracing::info!("设置页请求执行 GPU 兼容性检查");
                crate::event::emit(crate::event::Event::Window(
                    crate::event::window::Event::gpu_check_run(),
                ));
            }
            crate::settings::Event::CopyGpuDiagnostics => {
                let detail = match &root.settings.compat.check_state {
                    GpuCheckUiState::Done(result) => Some(result.detail.clone()),
                    _ => None,
                };
                if let Some(detail) = detail {
                    match arboard::Clipboard::new()
                        .and_then(|mut clipboard| clipboard.set_text(detail))
                    {
                        Ok(()) => {
                            root.settings.compat.copied = true;
                            tracing::info!("GPU 诊断信息已复制到剪贴板");
                        }
                        Err(e) => tracing::warn!("复制 GPU 诊断信息失败: {e}"),
                    }
                }
            }
            crate::settings::Event::AboutLogoClicked => {
                // 关于页 logo 彩蛋（UI-007）：点击指针位置已由 Host 在路由前写入
                // `state.about_egg`（`mouse_area::on_press` 不携带坐标）。
                let clicks_before = root.state.about_egg.clicks();
                let consumed = root.state.about_egg.on_logo_click(Instant::now());
                if consumed {
                    if root.state.about_egg.is_airborne() {
                        tracing::info!(
                            "关于页 logo 彩蛋触发：{} 连点（此前计数 {}）",
                            lumino_ui_core::state::about_egg::ABOUT_EGG_CLICK_THRESHOLD,
                            clicks_before + 1
                        );
                        // 预热音效（解码 + 开流并保持暂停）：序列约 1.7s，足够覆盖初始化，
                        // 落地瞬间只剩一次 `play`，避免开流延迟导致「落地后半天才响」。
                        lumino_midi_io::ui_sfx::prewarm(
                            root.settings.synth.selected_audio_output_device.as_deref(),
                        );
                    } else {
                        tracing::debug!(
                            "关于页 logo 单击反馈：计数 {}/{}",
                            root.state.about_egg.clicks(),
                            lumino_ui_core::state::about_egg::ABOUT_EGG_CLICK_THRESHOLD
                        );
                    }
                }
            }
            crate::settings::Event::MenuSelected(index)
                if index == crate::settings::MENU_INDEX_ABOUT =>
            {
                // 关于页回声洞彩蛋（UI-006）：打字机播放的**唯一装填点**。
                // 关于页永远不是设置面板的初始页（`SettingsPanel::new` 的
                // `selected_menu_index` 硬编码为 0），故「进入关于页」只能由菜单切换到达；
                // 重复进入等价于重播当前文案。
                root.state.echo_cave.begin_typing(Instant::now());
                tracing::debug!(
                    "关于页回声洞（UI-006）：进入关于页，开始打字机播放（第 {} 条）",
                    root.state.echo_cave.index() + 1
                );
            }
            crate::settings::Event::EchoCaveClicked => {
                // 关于页回声洞彩蛋（UI-006）：单击切换——当前内容闪烁退出，下一条打字机进入。
                // 闪烁期内的点击会被状态机忽略（返回 false），保证连点不会把闪烁无限推迟。
                let now = Instant::now();
                let consumed = root.state.echo_cave.on_click(now);
                if consumed {
                    tracing::debug!(
                        "关于页回声洞（UI-006）：点击切换，第 {} 条进入闪烁退出",
                        root.state.echo_cave.index() + 1
                    );
                }
            }
            _ => {} // 其他设置变更由 settings.update() 同步
        }

        None // 已处理
    }
}
