//! `SettingsPanel::update` 的分组事件处理
//!
//! 从 `panel.rs` 的单个超长 `match` 按 UI 设置区块拆出，各匹配臂逐字搬迁：
//! 事件处理顺序、状态读写顺序，以及所有默认值、范围、步进、单位与文案均未改动。
//!
//! 分组边界是设置页面的区块（音频后端 / 复音与 GPU / 编辑字体 / 视图 /
//! MIDI 设备 / 外观 / 编辑历史 / 兼容性 / 关于），不是按行数硬切。
//!
//! `panel.rs` 中的 `update` 仍对 `Event` 全变体做编译期穷尽匹配（无 `_` 通配臂），
//! 每个分组只接收已归类好的 `Event`；这里的 `_ => {}` 是不可达兜底，
//! 新增变体仍会在 `update` 的穷尽匹配处触发编译错误。

use crate::{Event, SettingsPanel};
use lumino_core::storage::config::SynthBackend;
use lumino_message::settings_event::OutputType;

use super::file_pickers;

/// 解析设置输入字符串并应用；解析失败记录 warn 并忽略（保持旧行为不变）。
///
/// 收敛 `update()` 中大量 `if let Ok(v) = s.parse::<T>()` 样板。
fn parse_setting<T>(raw: &str, apply: impl FnOnce(T))
where
    T: std::str::FromStr,
{
    match raw.parse::<T>() {
        Ok(v) => apply(v),
        Err(_) => tracing::warn!("设置输入非法，已忽略: '{}'", raw),
    }
}

impl SettingsPanel {
    /// 音频后端、音色库、标题栏与 XSynth 基础参数的事件处理。
    pub(super) fn update_audio_backend(&mut self, event: Event) {
        match event {
            Event::MenuSelected(idx) => {
                self.selected_menu_index = idx;
            }
            Event::SynthBackendChanged(backend) => {
                self.synth.backend = backend;
            }
            Event::OutputTypeChanged(ot) => {
                // 顶层输出类型：内置合成器归为一类，进入后由 SynthBackendChanged 选择具体引擎；
                // 从外部类型切回内置时，默认落到 XSynth（用户可在子下拉里改选 LGS）。
                self.synth.backend = match ot {
                    OutputType::Builtin => {
                        if matches!(
                            self.synth.backend,
                            SynthBackend::Kdmapi | SynthBackend::Maestro | SynthBackend::System
                        ) {
                            SynthBackend::XSynth
                        } else {
                            self.synth.backend
                        }
                    }
                    OutputType::Kdmapi => SynthBackend::Kdmapi,
                    OutputType::Maestro => SynthBackend::Maestro,
                    OutputType::System => SynthBackend::System,
                };
            }
            Event::AudioEngineChanged(kind) => {
                self.synth.audio_engine = kind;
            }
            Event::SoundfontPathChanged(path) => {
                self.synth.soundfont_path = path;
            }
            Event::BrowseSoundfont => {
                file_pickers::browse_soundfont(self);
            }
            Event::NativeTitlebarChanged(enabled) => {
                self.synth.use_native_titlebar = enabled;
            }
            Event::XSynthBufferChanged(ms) => {
                self.synth.xsynth_buffer_ms = ms;
            }
            Event::XSynthSampleRateChanged(sr) => {
                self.synth.xsynth_sample_rate = sr;
            }
            Event::XSynthMaxVoicesChanged(v) => {
                self.synth.xsynth_max_voices_per_key = v;
            }
            Event::XSynthMaxVoicesCustomInput(s) => {
                let t = s.trim();
                if t.is_empty() || t.eq_ignore_ascii_case("unlimited") || t == "0" {
                    self.synth.xsynth_max_voices_per_key = None;
                } else if let Ok(v) = t.parse::<usize>() {
                    if v == 0 {
                        self.synth.xsynth_max_voices_per_key = None;
                    } else {
                        self.synth.xsynth_max_voices_per_key = Some(v.clamp(1, 128));
                    }
                }
            }
            _ => {}
        }
    }

    /// 全局复音上限、LGS 块长/过滤与 GPU 状态指示的事件处理。
    pub(super) fn update_voice_performance(&mut self, event: Event) {
        match event {
            Event::XSynthGlobalVoiceLimitChanged(v) => {
                // 0 = 自动（引擎默认 10000）；其余夹紧到 [64, 65536]
                self.synth.xsynth_global_voice_limit = if v == 0 {
                    None
                } else {
                    Some(v.clamp(64, 65536))
                };
            }
            Event::XSynthVoiceTargetRatioChanged(r) => {
                self.synth.xsynth_voice_target_ratio = r.clamp(0.50, 0.90);
            }
            Event::XSynthSoftNpsGateChanged(on) => {
                self.synth.xsynth_soft_nps_gate = on;
            }
            Event::LgsBlockSizeChanged(size) => {
                // 仅接受 2 的幂（GUI 滑块已约束，这里兜底夹紧到 [64, 8192] 内的 2 的幂）
                let clamped = size.clamp(64, 8192);
                self.synth.lgs_block_size = clamped.next_power_of_two();
            }
            Event::LgsOfflineBlockSizeChanged(size) => {
                // REF-LGS-009：离线导出块长只接受 [512, 8192] 的 2 的幂
                // （GUI 是固定候选集，这里兜底防脏事件）。
                self.synth.lgs_offline_block_size = size.clamp(512, 8192).next_power_of_two();
            }
            Event::LgsMaxVoicesChanged(v) => {
                self.synth.lgs_max_voices_per_key = v.clamp(0, 128);
            }
            Event::LgsGlobalVoiceLimitChanged(v) => {
                // 0 = 自动（LGS_AUTO_MAX_VOICES=16384）；其余夹紧 [64, 65536]（与 CPU 同口径）
                self.synth.lgs_global_voice_limit = if v == 0 {
                    None
                } else {
                    Some(v.clamp(64, 65536))
                };
            }
            Event::LgsSoftNpsGateChanged(on) => {
                self.synth.lgs_soft_nps_gate = on;
            }
            Event::LgsVelocityFilterChanged(v) => {
                self.synth.lgs_velocity_filter_threshold = v.clamp(0, 127);
            }
            Event::GpuBufferStatusEnabledChanged(on) => {
                self.synth.gpu_buffer_status_enabled = on;
            }
            Event::GpuGovernorMetricsEnabledChanged(on) => {
                self.synth.gpu_governor_metrics_enabled = on;
            }
            Event::GpuLoadDisplayChanged(mode) => {
                self.synth.gpu_load_display = mode;
            }
            _ => {}
        }
    }

    /// 主题、橡皮擦、框选与程序字体的事件处理。
    pub(super) fn update_editing_fonts(&mut self, event: Event) {
        match event {
            Event::ThemeChanged(_) => {
                // 主题变更由外部处理
            }
            Event::EraserBehaviorChanged(behavior) => {
                self.editing.eraser_behavior = behavior;
            }
            Event::SelectionBoxModeChanged(mode) => {
                self.editing.selection_box_mode = mode;
            }
            Event::ProgramFontNameChanged(name) => {
                self.editing.program_font_name = name;
                // UI-001：与自定义字体文件路径互斥（选系统字体 → 清空路径）
                self.editing.program_font_path.clear();
            }
            Event::ProgramFontPathChanged(path) => {
                self.editing.program_font_path = path;
                // UI-001：与系统字体名互斥（输入/粘贴路径 → 清空字体名）
                if !self.editing.program_font_path.is_empty() {
                    self.editing.program_font_name.clear();
                }
            }
            Event::BrowseProgramFont => {
                file_pickers::browse_program_font(self);
            }
            _ => {}
        }
    }

    /// 自动滚动、力度过滤与钢琴卷帘显示的事件处理。
    pub(super) fn update_view_settings(&mut self, event: Event) {
        match event {
            // 自动滚动配置事件
            Event::AutoScrollFixedPositionChanged(value) => {
                parse_setting(&value, |val| self.auto_scroll.fixed_position = val);
            }
            Event::AutoScrollPageTriggerOffsetChanged(value) => {
                parse_setting(&value, |val| self.auto_scroll.page_trigger_offset = val);
            }
            Event::AutoScrollPageReturnPositionChanged(value) => {
                parse_setting(&value, |val| self.auto_scroll.page_return_position = val);
            }
            // 力度过滤
            Event::VelocityFilterThresholdChanged(value) => {
                parse_setting(&value, |val| self.midi.velocity_filter_threshold = val);
            }
            Event::IconHiDPIChanged(enabled) => {
                self.display.icon_hidpi = enabled;
            }
            Event::Enable256keyChanged(enabled) => {
                self.display.enable_256key = enabled;
            }
            Event::NoteCountDisplayChanged(display) => {
                self.display.note_count_display = display;
            }
            Event::VelocityCurveStyleChanged(enabled) => {
                self.display.velocity_curve_style = enabled;
            }
            _ => {}
        }
    }

    /// MIDI 输入设备、WinMM 播表与音频输出设备选择的事件处理。
    pub(super) fn update_midi_devices(&mut self, event: Event) {
        match event {
            Event::DeviceSelected(id) => {
                self.midi.selected_device = Some(id);
                tracing::debug!("设置: MIDI 输入设备选择为 #{}", id);
            }
            Event::ScanMidiDevices => {
                // 实际扫描由 editor/ui 的 Root::scan_midi_input_devices 执行并写回设置面板
                tracing::debug!("设置: 收到 MIDI 输入设备扫描请求");
            }
            Event::WinmmOutputSelected(id) => {
                self.midi.selected_winmm_output = Some(id);
                tracing::debug!("设置: WinMM 输出设备(播表)选择为 #{}", id);
            }
            Event::ScanWinmmOutputs => {
                // 实际扫描由 editor/ui 的 Root::scan_winmm_outputs 执行并写回设置面板
                tracing::debug!("设置: 收到 WinMM 播表扫描请求");
            }
            Event::AudioOutputSelected(name) => {
                // 空串/默认项表示使用系统默认输出设备
                self.synth.selected_audio_output_device =
                    if name.is_empty() { None } else { Some(name) };
                tracing::debug!(
                    "设置: 音频播放输出设备选择为 {:?}",
                    self.synth.selected_audio_output_device
                );
            }
            Event::ScanAudioOutputs => {
                // 实际扫描由 editor/ui 的 Root::scan_audio_outputs 执行并写回设置面板
                tracing::debug!("设置: 收到音频播放输出设备扫描请求");
            }
            _ => {}
        }
    }

    /// 界面语言、洋葱皮后端、播放键色、音轨行为与调色板的事件处理。
    pub(super) fn update_appearance(&mut self, event: Event) {
        match event {
            Event::LanguagePreferenceChanged(preference) => {
                // UI-018：偏好变更后立即解析出生效语言，界面本帧即切换；
                // 「跟随系统」实时探测，避免用户以为没生效。
                let language = preference.resolve();
                self.display.language_preference = preference;
                self.display.language = language;
                tracing::debug!("设置: 界面语言偏好切换为 {preference:?}（生效语言 {language}）");
            }
            // 洋葱皮渲染后端（加载 MIDI 后锁定，见 `backend_locked`）
            Event::OnionSkinBackendChanged(backend) => {
                // 锁定态（主窗口已加载 MIDI）忽略切换请求：后端选择必须在
                // 「没有 MIDI 工程」时做出，避免运行中切换导致缓存/渲染语义错位。
                if self.onion_skin.backend_locked {
                    tracing::debug!("设置: 洋葱皮渲染方式已锁定，忽略切换请求 {backend:?}");
                } else {
                    self.onion_skin.backend = backend;
                }
            }
            Event::PlaybackKeyColorsEnabledChanged(v) => {
                self.display.playback_key_colors_enabled = v;
            }
            Event::TrackAddBehaviorChanged(v) => {
                self.editing.track_add_behavior = v;
            }
            Event::PaletteChanged(name) => {
                if let Some(p) = self.display.available_palettes.iter().find(|n| **n == name) {
                    self.display.selected_palette = p.to_string();
                    tracing::debug!("设置: 调色板切换为 '{}'", name);
                }
            }
            _ => {}
        }
    }

    /// 编辑历史、自动保存、自动化显示与监控刷新的事件处理。
    pub(super) fn update_edit_history(&mut self, event: Event) {
        match event {
            // 编辑设置
            Event::HistoryTotalLimitChanged(s) => {
                parse_setting(&s, |v: usize| {
                    self.editing.history_total_limit = v.clamp(10, 1000)
                });
            }
            Event::HistoryEntryLimitChanged(s) => {
                parse_setting(&s, |v: usize| {
                    self.editing.history_entry_limit = v.clamp(100, 10000)
                });
            }
            Event::MergeWindowMsChanged(s) => {
                parse_setting(&s, |v: u64| self.editing.merge_window_ms = v.min(5000));
            }
            Event::InterceptNotificationChanged(enabled) => {
                self.editing.intercept_notification_enabled = enabled;
            }
            Event::AutosaveEnabledChanged(enabled) => {
                self.editing.autosave_enabled = enabled;
            }
            Event::AutosaveIntervalMinsChanged(s) => {
                // 输入框原文先存面板（保证输入体验）；数值合法性在落盘 patch 时
                // 统一 parse + clamp，非法输入不生效。
                self.editing.autosave_interval_mins = s;
            }
            Event::AutomationLineThicknessChanged(v) => {
                self.editing.automation_line_thickness = v.clamp(1.0, 10.0);
            }
            Event::TempoMaxBpmChanged(v) => {
                self.editing.tempo_max_bpm = v;
            }
            // DRAW-045：自定义输入已迁到独立对话框，面板只保留生效值。
            Event::TempoMaxBpmCustomOpen => {}
            Event::LogRetentionCountChanged(s) => {
                parse_setting(&s, |v: usize| self.logging.log_retention_count = v);
            }
            Event::MonitorRefreshIntervalChanged(v) => {
                self.logging.monitor_refresh_interval_ms = v.clamp(50.0, 2000.0);
            }
            Event::FirstScreenOpacityChanged(v) => {
                self.display.first_screen_opacity = v.clamp(0.0, 1.0);
            }
            _ => {}
        }
    }

    /// GPU 兼容性检查与诊断复制的事件处理。
    pub(super) fn update_compatibility(&mut self, event: Event) {
        match event {
            // 兼容性设置
            Event::GpuCheckOnStartupChanged(enabled) => {
                self.compat.check_on_startup = enabled;
            }
            Event::GpuWarningSuppressedChanged(suppressed) => {
                self.compat.warning_suppressed = suppressed;
            }
            Event::DominoClipboardEnabledChanged(enabled) => {
                self.compat.domino_clipboard_enabled = enabled;
            }
            Event::RunGpuCompatibilityCheck => {
                // 检测状态由 Runner 单点裁决并回推（在飞 / 冷却 / 会话锁定会拒绝），
                // 面板不本地置 Running——否则关窗重开时面板状态会与真源分叉。
                self.compat.copied = false;
            }
            Event::CopyGpuDiagnostics => {
                // 剪贴板写入由 lumino-ui 的处理器完成（ui-settings 不依赖剪贴板库）
            }
            _ => {}
        }
    }

    /// 关于页两处彩蛋事件的穷尽匹配占位（动画状态均归主窗口持有）。
    pub(super) fn update_about(&mut self, event: Event) {
        match event {
            Event::AboutLogoClicked => {
                // 关于页 logo 彩蛋（UI-007）：动画状态归属 `RootState::about_egg`
                // （设置面板每次打开都会重建，面板内状态无法承载跨开关的消失语义），
                // 本面板不持有该状态，故此处仅作为穷尽匹配的占位。
            }
            Event::EchoCaveClicked => {
                // 关于页回声洞彩蛋（UI-006）：动画状态归属 `RootState::echo_cave`——
                // 若放面板内，`Root::apply_settings` 的整面板替换会把闪烁中的瞬时状态
                // 搬进主窗口，使主窗口为一段与它无关的动画持续重绘（见
                // `ui/src/root/editor_ops/dialog/settings.rs`）。故此处同样是穷尽匹配占位。
            }
            _ => {}
        }
    }
}
