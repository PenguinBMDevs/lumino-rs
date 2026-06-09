//! 对话框管理处理器

use crate::host::DialogResult;
use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 对话框消息处理器
pub struct DialogHandler;

impl DialogHandler {
    pub fn new() -> Self {
        Self
    }

    fn handle_custom_precision_dialog_open(&self, _root: &mut Root) {
        tracing::info!("Root: 请求打开自定义精度对话框");
        lumino_core::event::emit(lumino_core::event::Event::Window(
            lumino_core::event::window::Event::OpenCustomPrecisionDialog,
        ));
    }

    fn handle_custom_precision_dialog_close(&self, root: &mut Root) {
        root.state.custom_precision_dialog.is_open = false;
        root.state.dialog_result = Some(DialogResult::Cancel);
    }

    fn handle_confirm_custom_precision(&self, root: &mut Root) {
        let dialog = &root.state.custom_precision_dialog;

        if dialog.calculate_ticks(1).is_none() {
            tracing::warn!("自定义精度: 无效的输入值");
            return;
        }

        // 设置对话框结果，由 runner 在主窗口应用精度
        let denominator = match (
            dialog.note_value.parse::<f32>(),
            dialog.divisor.parse::<f32>(),
        ) {
            (Ok(nv), Ok(div)) if nv > 0.0 && div > 0.0 => (nv * div).to_string(),
            _ => {
                tracing::warn!("自定义精度: 无法解析 note_value/divisor");
                return;
            }
        };

        root.state.dialog_result = Some(DialogResult::CustomPrecision {
            numerator: dialog.tuplet_count.clone(),
            denominator,
        });
        root.state.custom_precision_dialog.is_open = false;
        tracing::info!("自定义精度已提交，等待应用");
    }

    fn update_precision_if_digit(target: &mut String, value: &str) {
        if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
            *target = value.to_string();
        }
    }

    fn handle_confirm_project_settings(&self, root: &mut Root) {
        let dialog = &root.state.project_settings_dialog;

        // 验证 BPM 值
        if let Some(tempo) = dialog.parse_tempo() {
            let title = dialog.title.clone();
            let copyright = dialog.copyright.clone();

            // 设置对话框结果（触发窗口关闭 + 主窗口处理）
            root.state.dialog_result = Some(DialogResult::ProjectSettings {
                title,
                tempo,
                copyright,
            });
            root.state.project_settings_dialog.is_open = false;
        } else {
            tracing::warn!("工程设置: BPM 值无效: {}", dialog.tempo);
        }
    }
}

impl Default for DialogHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for DialogHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::OpenCustomPrecisionDialog => {
                self.handle_custom_precision_dialog_open(root);
                None
            }
            Message::CloseCustomPrecisionDialog => {
                self.handle_custom_precision_dialog_close(root);
                None
            }
            Message::ConfirmCustomPrecision => {
                self.handle_confirm_custom_precision(root);
                None
            }
            Message::CustomPrecisionTupletCountChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.tuplet_count,
                    &value,
                );
                None
            }
            Message::CustomPrecisionTupletTypeChanged(value) => {
                root.state.custom_precision_dialog.tuplet_type = value;
                root.state.custom_precision_dialog.tuplet_count = value.value().to_string();
                None
            }
            Message::CustomPrecisionDotTypeChanged(value) => {
                root.state.custom_precision_dialog.dot_type = value;
                None
            }
            Message::CustomPrecisionNoteValueChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.note_value,
                    &value,
                );
                None
            }
            Message::CustomPrecisionDivisorChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.divisor,
                    &value,
                );
                None
            }
            Message::ConfirmLoadConfirm => {
                root.handle_confirm_load();
                None
            }
            Message::CloseLoadConfirmDialog => {
                root.handle_cancel_load();
                None
            }
            // 工程设置对话框消息
            Message::OpenProjectSettingsDialog => {
                root.state.project_settings_dialog.is_open = true;
                None
            }
            Message::CloseProjectSettingsDialog => {
                root.state.project_settings_dialog.is_open = false;
                root.state.dialog_result = Some(DialogResult::Cancel);
                None
            }
            Message::ConfirmProjectSettings => {
                self.handle_confirm_project_settings(root);
                None
            }
            Message::ProjectSettingsTitleChanged(value) => {
                root.state.project_settings_dialog.title = value;
                None
            }
            Message::ProjectSettingsTempoChanged(value) => {
                // 只允许数字和小数点
                if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    root.state.project_settings_dialog.tempo = value;
                }
                None
            }
            Message::ProjectSettingsCopyrightChanged(value) => {
                root.state.project_settings_dialog.copyright = value;
                None
            }
            // 设置对话框消息
            Message::OpenSettingsDialog => {
                root.state.dialog_type = crate::state::root_state::DialogType::Settings;
                None
            }
            Message::CloseSettingsDialog => {
                // 返回设置结果，将设置同步到主窗口
                root.state.dialog_result = Some(DialogResult::Settings {
                    settings: root.settings.clone(),
                    theme: root.window.theme.to_string(),
                });
                None
            }

            // 音频导出对话框消息
            Message::OpenAudioExportDialog => {
                root.state.dialog_type = crate::state::root_state::DialogType::AudioExport;
                None
            }
            Message::CloseAudioExportDialog => {
                root.state.audio_export_dialog.is_open = false;
                None
            }
            Message::AudioExportProjectNameChanged(value) => {
                root.state.audio_export_dialog.project_name = value;
                None
            }
            Message::AudioExportOutputPathChanged(value) => {
                root.state.audio_export_dialog.output_path = value;
                None
            }
            Message::AudioExportFormatChanged(value) => {
                root.state.audio_export_dialog.format = value;
                None
            }
            Message::AudioExportSampleRateChanged(value) => {
                root.state.audio_export_dialog.sample_rate = value;
                None
            }
            Message::AudioExportChannelsChanged(value) => {
                root.state.audio_export_dialog.channels = value;
                None
            }
            Message::AudioExportLayersChanged(value) => {
                // 只允许数字
                if value.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(v) = value.parse::<u32>() {
                        root.state.audio_export_dialog.layers = v;
                    }
                }
                None
            }
            Message::AudioExportChannelThreadingChanged(value) => {
                root.state.audio_export_dialog.channel_threading = value;
                None
            }
            Message::AudioExportKeyThreadingChanged(value) => {
                root.state.audio_export_dialog.key_threading = value;
                None
            }
            Message::AudioExportInterpolationChanged(value) => {
                root.state.audio_export_dialog.interpolation = value;
                None
            }
            Message::AudioExportApplyLimiterChanged(value) => {
                root.state.audio_export_dialog.apply_limiter = value;
                None
            }
            Message::AudioExportDisableFadeOutChanged(value) => {
                root.state.audio_export_dialog.disable_fade_out = value;
                None
            }
            Message::AudioExportLinearEnvelopeChanged(value) => {
                root.state.audio_export_dialog.linear_envelope = value;
                None
            }
            Message::AudioExportBrowseOutput => {
                // 使用原生文件对话框选择输出路径
                let current = root.state.audio_export_dialog.output_path.clone();
                let default_name = std::path::Path::new(&current)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("export.wav");
                let default_dir = std::path::Path::new(&current)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or(".");
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(default_name)
                    .set_directory(default_dir)
                    .add_filter("WAV 文件", &["wav"])
                    .add_filter("FLAC 文件", &["flac"])
                    .save_file()
                {
                    root.state.audio_export_dialog.output_path = path.to_string_lossy().to_string();
                }
                None
            }
            Message::AudioExportConfirm => {
                let state = &root.state.audio_export_dialog;
                root.state.dialog_result = Some(DialogResult::AudioExport {
                    project_name: state.project_name.clone(),
                    midi_path: state.midi_path.clone(),
                    soundfont_path: state.soundfont_path.clone(),
                    output_path: state.output_path.clone(),
                    sample_rate: state.sample_rate,
                    channels: state.channels,
                    layers: state.layers,
                    channel_threading: state.channel_threading,
                    key_threading: state.key_threading,
                    apply_limiter: state.apply_limiter,
                    disable_fade_out: state.disable_fade_out,
                    linear_envelope: state.linear_envelope,
                    interpolation: state.interpolation,
                    format: state.format,
                });
                root.state.audio_export_dialog.is_open = false;
                None
            }
            Message::AudioExportCancel => {
                root.state.audio_export_dialog.is_open = false;
                root.state.dialog_result = Some(DialogResult::Cancel);
                None
            }
            // 音符变速对话框消息
            Message::OpenSpeedChangeDialog => {
                root.state.speed_change_dialog.is_open = true;
                None
            }
            Message::CloseSpeedChangeDialog => {
                root.state.speed_change_dialog.is_open = false;
                root.state.dialog_result = Some(DialogResult::Cancel);
                None
            }
            Message::ConfirmSpeedChange => {
                if let Some(factor) = root.state.speed_change_dialog.parse_factor() {
                    root.toolbar.speed_factor = factor;
                    tracing::info!("Root: 速度因子已更新为 {}", factor);
                    // 设置对话框结果（用于独立窗口模式）
                    root.state.dialog_result = Some(DialogResult::SpeedChange { factor });
                    // 必须有选中音符才能执行变速
                    if !root
                        .editor
                        .editor_state
                        .interaction
                        .selected_notes
                        .is_empty()
                    {
                        let modified = root.editor.apply_speed_change(factor);
                        if modified > 0 {
                            tracing::info!("Root: 变速完成，修改了 {} 个音符", modified);
                            root.update_playback_notes();
                            root.editor.clear_notes_changed();
                        }
                    } else {
                        tracing::warn!("Root: 没有选中音符，不执行变速对话框的变速操作");
                    }
                } else {
                    tracing::warn!(
                        "Root: 无效的速度因子输入: {}",
                        root.state.speed_change_dialog.factor_input
                    );
                }
                root.state.speed_change_dialog.is_open = false;
                None
            }
            Message::SpeedChangeFactorChanged(value) => {
                root.state.speed_change_dialog.factor_input = value;
                None
            }

            other => Some(other),
        }
    }
}
