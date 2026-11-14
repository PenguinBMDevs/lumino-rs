//! 音频导出面板：生产状态 → egui 模型，以及动作 → `Message` 映射。
//!
//! 该面板替换主界面整块右侧内容区（工具栏 / 画布 / 状态栏），不是独立对话框；
//! 矩形的计算在宿主（`Host::render_egui_overlay`）里完成并随模型下发。

use lumino_core::{tr, trf};
use lumino_message::{AudioBackend, AudioChannels, AudioFormat, Interpolation, ThreadingOption};
use lumino_ui_egui::chrome::{
    AudioButtonsMode, AudioButtonsModel, AudioChoice, AudioExportModel, AudioFilterModel,
    AudioOutputModel, AudioProjectModel, AudioSettingsModel, AudioToggle, AudioToggleId,
    ExportMessageKind,
};

use crate::Message;

/// 固定选项表（下标即动作回传值）。
const BACKENDS: [AudioBackend; 2] = [AudioBackend::Cpu, AudioBackend::Gpu];
const FORMATS: [AudioFormat; 5] = [
    AudioFormat::WAV,
    AudioFormat::FLAC,
    AudioFormat::MP3,
    AudioFormat::Ogg,
    AudioFormat::WavPack,
];
const SAMPLE_RATES: [u32; 4] = [22050, 44100, 48000, 96000];
const CHANNELS: [AudioChannels; 2] = [AudioChannels::Mono, AudioChannels::Stereo];
const THREADING: [ThreadingOption; 5] = [
    ThreadingOption::None,
    ThreadingOption::Auto,
    ThreadingOption::Manual(2),
    ThreadingOption::Manual(4),
    ThreadingOption::Manual(8),
];
const INTERPOLATIONS: [Interpolation; 2] = [Interpolation::None, Interpolation::Linear];

fn choices<T: PartialEq + std::fmt::Display>(items: &[T], selected: T) -> Vec<AudioChoice> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| AudioChoice {
            index,
            label: item.to_string(),
            selected: *item == selected,
        })
        .collect()
}

impl crate::root::Root {
    /// 音频导出模型：六个区块全部由生产状态与翻译表填充。
    pub(crate) fn audio_export_model(&self) -> AudioExportModel {
        let state = &self.state.audio_export_dialog;
        let gpu = state.backend == AudioBackend::Gpu;

        let toggles = vec![
            AudioToggle {
                id: AudioToggleId::ApplyLimiter,
                label: if gpu {
                    tr!("启用限幅器（GPU 引擎始终限幅，此选项对 GPU 无效）").to_string()
                } else {
                    tr!("启用限幅器（大复音防削波，会轻微影响响度，建议开启）").to_string()
                },
                checked: gpu || state.apply_limiter,
                enabled: !gpu,
            },
            AudioToggle {
                id: AudioToggleId::DisableFadeOut,
                label: if gpu {
                    tr!("禁用淡出（GPU 引擎固定 1ms 淡出，此选项对 GPU 无效）").to_string()
                } else {
                    tr!("禁用淡出（voice 被抢占时硬切，会产生咔哒声，不建议开启）").to_string()
                },
                checked: !gpu && state.disable_fade_out,
                enabled: !gpu,
            },
            AudioToggle {
                id: AudioToggleId::LinearEnvelope,
                label: tr!("线性包络（CPU 衰减/释音用线性，GPU 默认线性，关闭则全指数）")
                    .to_string(),
                checked: state.linear_envelope,
                enabled: true,
            },
        ];

        AudioExportModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("音频导出").to_string(),
            project: AudioProjectModel {
                name_label: tr!("工程名称").to_string(),
                name: state.project_name.clone(),
                name_hint: tr!("工程名称").to_string(),
                midi_label: tr!("MIDI 路径").to_string(),
                midi_path: state.midi_path.clone(),
                soundfont_label: tr!("音色库 (SF2)").to_string(),
                soundfont_path: state.soundfont_path.clone(),
                browse: tr!("浏览...").to_string(),
            },
            settings: AudioSettingsModel {
                title: tr!("音频设置").to_string(),
                backend_label: tr!("渲染后端:").to_string(),
                backends: choices(&BACKENDS, state.backend),
                backend_hint: tr!(
                    "CPU 兼容性最好；GPU 需 Vulkan/Metal，无适配器时自动回退到 CPU，速度提升 2-5 倍"
                )
                .to_string(),
                format_label: tr!("输出格式:").to_string(),
                formats: choices(&FORMATS, state.format),
                format_hint: tr!("仅 MP3/Ogg 有效；WAV/FLAC 忽略此项").to_string(),
                bitrate_label: tr!("比特率 (kbps):").to_string(),
                bitrate: state.audio_bitrate.to_string(),
                bitrate_hint: tr!("仅 MP3/Ogg 有效；WAV/FLAC 忽略此项").to_string(),
                sample_rate_label: tr!("采样率 (Hz):").to_string(),
                sample_rates: choices(&SAMPLE_RATES, state.sample_rate),
                sample_rate_hint: tr!(
                    "MP3 最高 48000 Hz，FLAC 最高 384000 Hz，超出会自动报错"
                )
                .to_string(),
                channels_label: tr!("通道数:").to_string(),
                channels: choices(&CHANNELS, state.channels),
                layers_label: tr!("每键复音 (0=不限):").to_string(),
                layers: state.layers.to_string(),
                layers_hint: tr!(
                    "每个键（通道+音高）最大同时发声数；0=不限（不推荐）。建议保持 32：实测同素材全曲 32 → 165s、0 → 59min（声部无限堆积，耗时与内存暴涨）。CPU 按 voice 计、GPU 按音符组计。"
                )
                .to_string(),
                layers_warning: (state.layers == 0).then(|| {
                    tr!(
                        "已设为不限：密集素材下声部会无限堆积，渲染时间与内存会显著恶化——除非明确需要，建议改回 32。"
                    )
                    .to_string()
                }),
                channel_thread_label: tr!("通道多线程 (CPU):").to_string(),
                channel_threads: choices(&THREADING, state.channel_threading),
                key_thread_label: tr!("按键多线程 (CPU):").to_string(),
                key_threads: choices(&THREADING, state.key_threading),
                interpolation_label: tr!("插值算法:").to_string(),
                interpolations: choices(&INTERPOLATIONS, state.interpolation),
                interpolation_hint: tr!(
                    "无插值有混叠，线性为默认（GPU 固定线性，与 CPU 线性一致）"
                )
                .to_string(),
                toggles,
            },
            filter: AudioFilterModel {
                title: tr!("事件过滤").to_string(),
                ignore: AudioToggle {
                    id: AudioToggleId::IgnoreProgram,
                    label: tr!("忽略音色变化事件（Program Change）").to_string(),
                    checked: state.ignore_program_changes,
                    enabled: true,
                },
                ignore_hint: tr!("勾选后所有音色切换事件被丢弃，所有音符用默认音色渲染")
                    .to_string(),
                velocity_title: tr!("音符力度过滤（仅保留指定力度范围内的音符）").to_string(),
                velocity_toggle: AudioToggle {
                    id: AudioToggleId::FilterVelocity,
                    label: tr!("启用力度过滤（超出范围的音符将被静音丢弃）").to_string(),
                    checked: state.filter_velocity,
                    enabled: true,
                },
                velocity_range_label: tr!("力度范围 (0-127):").to_string(),
                velocity_low: state.velocity_low.to_string(),
                velocity_high: state.velocity_high.to_string(),
                key_title: tr!(
                    "音符键位过滤（仅保留指定键位范围内的音符；0=C-1, 60=C4, 127=G9）"
                )
                .to_string(),
                key_toggle: AudioToggle {
                    id: AudioToggleId::FilterKey,
                    label: tr!("启用键位过滤（超出范围的音符将被静音丢弃）").to_string(),
                    checked: state.filter_key,
                    enabled: true,
                },
                key_range_label: tr!("键位范围 (0-127):").to_string(),
                key_low: state.key_low.to_string(),
                key_high: state.key_high.to_string(),
                delay_label: tr!("音符结束延迟 (ms):").to_string(),
                delay: state.note_force_end_delay.to_string(),
                delay_hint: tr!(
                    "每个音符 NoteOff 后额外延长该时长再释放，>0 可避免极短音被吞；0=禁用。"
                )
                .to_string(),
            },
            output: AudioOutputModel {
                title: tr!("输出路径").to_string(),
                path: state.output_path.clone(),
                path_hint: tr!("选择输出路径...").to_string(),
                browse: tr!("浏览...").to_string(),
            },
            buttons: AudioButtonsModel {
                mode: buttons_mode(state),
            },
        }
    }

    /// egui 音频导出动作 → 生产 `Message`。
    pub(crate) fn audio_export_action_message(
        &self,
        action: &lumino_ui_egui::audio_export::AudioExportAction,
    ) -> Option<Message> {
        use crate::message::AudioExportAction as Prod;
        use lumino_ui_egui::audio_export::AudioExportAction as Action;
        let action = match action {
            Action::ProjectNameChanged(text) => Prod::ProjectNameChanged(text.clone()),
            Action::BrowseMidi => Prod::BrowseMidi,
            Action::BrowseSoundfont => Prod::BrowseSoundfont,
            Action::BrowseOutput => Prod::BrowseOutput,
            Action::OutputPathChanged(text) => Prod::OutputPathChanged(text.clone()),
            Action::BackendChanged(index) => Prod::BackendChanged(*BACKENDS.get(*index)?),
            Action::FormatChanged(index) => Prod::FormatChanged(*FORMATS.get(*index)?),
            Action::BitrateChanged(text) => Prod::BitrateChanged(text.clone()),
            Action::SampleRateChanged(index) => Prod::SampleRateChanged(*SAMPLE_RATES.get(*index)?),
            Action::ChannelsChanged(index) => Prod::ChannelsChanged(*CHANNELS.get(*index)?),
            Action::LayersChanged(text) => Prod::LayersChanged(text.clone()),
            Action::ChannelThreadingChanged(index) => {
                Prod::ChannelThreadingChanged(*THREADING.get(*index)?)
            }
            Action::KeyThreadingChanged(index) => {
                Prod::KeyThreadingChanged(*THREADING.get(*index)?)
            }
            Action::InterpolationChanged(index) => {
                Prod::InterpolationChanged(*INTERPOLATIONS.get(*index)?)
            }
            Action::Toggle(id, value) => match id {
                AudioToggleId::ApplyLimiter => Prod::ApplyLimiterChanged(*value),
                AudioToggleId::DisableFadeOut => Prod::DisableFadeOutChanged(*value),
                AudioToggleId::LinearEnvelope => Prod::LinearEnvelopeChanged(*value),
                AudioToggleId::IgnoreProgram => Prod::IgnoreProgramChangesChanged(*value),
                AudioToggleId::FilterVelocity => Prod::FilterVelocityChanged(*value),
                AudioToggleId::FilterKey => Prod::FilterKeyChanged(*value),
            },
            Action::VelocityLowChanged(text) => Prod::VelocityLowChanged(text.clone()),
            Action::VelocityHighChanged(text) => Prod::VelocityHighChanged(text.clone()),
            Action::KeyLowChanged(text) => Prod::KeyLowChanged(text.clone()),
            Action::KeyHighChanged(text) => Prod::KeyHighChanged(text.clone()),
            Action::NoteForceEndDelayChanged(text) => Prod::NoteForceEndDelayChanged(text.clone()),
            Action::Confirm => Prod::Confirm,
            Action::ClosePanel => Prod::ClosePanel,
            Action::TogglePause => Prod::TogglePause,
            Action::Abort => Prod::Abort,
            Action::ResetRendering => Prod::ResetRendering,
        };
        Some(Message::AudioExport(action))
    }
}

/// 按钮区三态。
fn buttons_mode(state: &lumino_ui_core::state::AudioExportDialogState) -> AudioButtonsMode {
    if state.is_rendering {
        return AudioButtonsMode::Rendering {
            status: if state.is_paused {
                tr!("已暂停").to_string()
            } else {
                state.render_message.clone()
            },
            progress: state.render_progress as f32,
            percentage: format!("{:.1}%", state.render_progress * 100.0),
            pause: if state.is_paused {
                tr!("继续").to_string()
            } else {
                tr!("暂停").to_string()
            },
            abort: tr!("中止").to_string(),
        };
    }
    if state.render_completed || state.render_error.is_some() {
        let (status, kind) = if state.render_completed {
            (tr!("导出完成").to_string(), ExportMessageKind::Completed)
        } else if let Some(err) = &state.render_error {
            if err == "已中止" {
                (tr!("已中止").to_string(), ExportMessageKind::Normal)
            } else {
                (trf!("导出失败: {err}", err = err), ExportMessageKind::Error)
            }
        } else {
            (state.render_message.clone(), ExportMessageKind::Normal)
        };
        return AudioButtonsMode::Finished {
            status,
            kind,
            reset: tr!("重置").to_string(),
            close: tr!("关闭").to_string(),
        };
    }
    AudioButtonsMode::Actions {
        close: tr!("关闭").to_string(),
        export: tr!("导出").to_string(),
    }
}
