//! 对话框管理处理器

use crate::host::DialogResult;
use crate::message::{AudioExportAction, Message, SpeedChangeAction, VideoExportAction};
use crate::root::Root;
use crate::root::handlers::MessageHandler;
use crate::state::root_state::VideoExportOverlayState;

/// 对话框消息处理器
pub struct DialogHandler;

impl DialogHandler {
    pub fn new() -> Self {
        Self
    }

    fn handle_custom_precision_dialog_open(&self, _root: &mut Root) {
        tracing::info!("Root: 请求打开自定义精度对话框");
        crate::event::emit(crate::event::Event::Window(
            crate::event::window::Event::open_custom_precision_dialog(),
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

            // 音频导出面板消息（主界面侧边栏面板，非独立对话框）
            Message::AudioExport(action) => {
                use AudioExportAction as A;
                match action {
                    A::OpenPanel => {
                        root.sidebar.audio_export_visible = true;
                        root.sidebar.route = crate::sidebar::Route::AudioExport;
                    }
                    A::ClosePanel => {
                        root.sidebar.audio_export_visible = false;
                        root.sidebar.route = crate::sidebar::Route::Arrangement;
                    }
                    A::Confirm => {
                        // 立即设置渲染状态（进度条第一时间刷新）
                        root.state.audio_export_dialog.is_rendering = true;
                        root.state.audio_export_dialog.render_completed = false;
                        root.state.audio_export_dialog.render_error = None;
                        root.state.audio_export_dialog.render_progress = 0.0;
                        root.state.audio_export_dialog.render_message = "正在初始化...".to_string();

                        // 从 dialog state 读取配置，发送事件到 runner
                        let st = &root.state.audio_export_dialog;

                        // 检查内存中是否有 MidiDocument
                        let document = root.midi.document.as_ref().map(|doc| {
                            tracing::info!("使用内存中的 MidiDocument 进行音频导出（零拷贝模式）");
                            std::sync::Arc::clone(doc)
                        });

                        if document.is_none() {
                            tracing::info!(
                                "内存中没有 MidiDocument，使用文件模式: {:?}",
                                st.midi_path
                            );
                        }

                        let ev = crate::event::window::Event::start_audio_export(
                            st.midi_path.clone(),
                            st.soundfont_path.clone(),
                            st.output_path.clone(),
                            st.sample_rate,
                            format!("{:?}", st.channels),
                            st.layers,
                            format!("{:?}", st.channel_threading),
                            format!("{:?}", st.key_threading),
                            format!("{:?}", st.interpolation),
                            st.apply_limiter,
                            st.disable_fade_out,
                            st.linear_envelope,
                            document,
                        );
                        crate::event::emit(crate::event::Event::Window(ev));
                    }
                    A::ProjectNameChanged(value) => {
                        root.state.audio_export_dialog.project_name = value;
                    }
                    A::OutputPathChanged(value) => {
                        root.state.audio_export_dialog.output_path = value;
                    }
                    A::FormatChanged(value) => {
                        root.state.audio_export_dialog.format = value;
                    }
                    A::SampleRateChanged(value) => {
                        root.state.audio_export_dialog.sample_rate = value;
                    }
                    A::ChannelsChanged(value) => {
                        root.state.audio_export_dialog.channels = value;
                    }
                    A::LayersChanged(value) => {
                        // 只允许数字
                        if value.chars().all(|c| c.is_ascii_digit())
                            && let Ok(v) = value.parse::<u32>()
                        {
                            root.state.audio_export_dialog.layers = v;
                        }
                    }
                    A::ChannelThreadingChanged(value) => {
                        root.state.audio_export_dialog.channel_threading = value;
                    }
                    A::KeyThreadingChanged(value) => {
                        root.state.audio_export_dialog.key_threading = value;
                    }
                    A::InterpolationChanged(value) => {
                        root.state.audio_export_dialog.interpolation = value;
                    }
                    A::ApplyLimiterChanged(value) => {
                        root.state.audio_export_dialog.apply_limiter = value;
                    }
                    A::DisableFadeOutChanged(value) => {
                        root.state.audio_export_dialog.disable_fade_out = value;
                    }
                    A::LinearEnvelopeChanged(value) => {
                        root.state.audio_export_dialog.linear_envelope = value;
                    }
                    A::BrowseOutput => {
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
                            root.state.audio_export_dialog.output_path =
                                path.to_string_lossy().to_string();
                        }
                    }
                    A::BrowseMidi => {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
                            .add_filter("MIDI 文件", &["mid", "midi"])
                            .add_filter("Lumino 项目", &["lmpj"])
                            .add_filter("Domino 项目", &["dms"])
                            .add_filter("所有文件", &["*"])
                            .pick_file()
                        {
                            root.state.audio_export_dialog.midi_path =
                                path.to_string_lossy().to_string();
                        }
                    }
                    A::BrowseSoundfont => {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("音色库文件", &["sf2", "sfz"])
                            .add_filter("SF2 文件", &["sf2"])
                            .add_filter("SFZ 文件", &["sfz"])
                            .add_filter("所有文件", &["*"])
                            .pick_file()
                        {
                            root.state.audio_export_dialog.soundfont_path =
                                path.to_string_lossy().to_string();
                        }
                    }
                    A::StartRendering => {
                        root.state.audio_export_dialog.is_rendering = true;
                        root.state.audio_export_dialog.render_completed = false;
                        root.state.audio_export_dialog.render_error = None;
                        root.state.audio_export_dialog.render_progress = 0.0;
                        root.state.audio_export_dialog.render_message = "正在初始化...".to_string();
                    }
                    A::UpdateRenderProgress { message, progress } => {
                        root.state.audio_export_dialog.render_message = message;
                        root.state.audio_export_dialog.render_progress = progress;
                    }
                    A::RenderCompleted => {
                        root.state.audio_export_dialog.is_rendering = false;
                        root.state.audio_export_dialog.render_completed = true;
                        root.state.audio_export_dialog.render_progress = 1.0;
                        root.state.audio_export_dialog.render_message = "导出完成".to_string();
                    }
                    A::RenderFailed(error) => {
                        root.state.audio_export_dialog.is_rendering = false;
                        root.state.audio_export_dialog.render_error = Some(error.clone());
                        root.state.audio_export_dialog.render_message =
                            format!("导出失败: {error}");
                    }
                    A::ResetRendering => {
                        root.state.audio_export_dialog.is_rendering = false;
                        root.state.audio_export_dialog.render_completed = false;
                        root.state.audio_export_dialog.render_error = None;
                        root.state.audio_export_dialog.render_progress = 0.0;
                        root.state.audio_export_dialog.render_message.clear();
                    }
                }
                None
            }
            // 视频导出面板消息
            Message::VideoExport(action) => {
                use VideoExportAction as V;
                match action {
                    V::OpenPanel => {
                        root.sidebar.video_export_visible = true;
                        root.sidebar.route = crate::sidebar::Route::VideoExport;
                    }
                    V::ClosePanel => {
                        root.sidebar.video_export_visible = false;
                        root.sidebar.route = crate::sidebar::Route::Arrangement;
                    }
                    V::StartExport => {
                        let st = &root.state.video_export_dialog;
                        let document = root.midi.document.as_ref().map(std::sync::Arc::clone);
                        // 先 clone 配置值，避免借用冲突
                        let output_path = st.output_path.clone();
                        let width = st.width;
                        let height = st.height;
                        let fps = st.fps;
                        let container = st.container.clone();
                        let codec = st.codec.clone();
                        let backend = st.backend.clone();
                        let quality = st.quality.clone();

                        // 设置导出中状态
                        root.state.video_export_dialog.overlay = VideoExportOverlayState::Exporting;
                        root.state.video_export_dialog.progress = 0.0;
                        root.state.video_export_dialog.status_message = "正在初始化...".to_string();
                        root.state.video_export_dialog.current_frame = 0;
                        root.state.video_export_dialog.total_frames = 0;
                        root.state.video_export_dialog.render_fps = 0.0;

                        let ev = crate::event::window::Event::start_video_export(
                            output_path,
                            width,
                            height,
                            fps,
                            container,
                            codec,
                            backend,
                            quality,
                            root.editor.editor_state.view.ppq,
                            root.editor.editor_state.view.visible_key_count,
                            document,
                        );
                        crate::event::emit(crate::event::Event::Window(ev));
                    }
                    V::CancelExport => {
                        root.state.video_export_dialog.overlay = VideoExportOverlayState::None;
                        root.state.video_export_dialog.preview_frame = None;
                        // 在对话框窗口中关闭窗口
                        if root.state.is_dialog_window {
                            root.state.dialog_result = Some(DialogResult::Cancel);
                        }
                        // 通知 Runner 取消导出（关闭对话框 → 设置取消标志 → 后台线程退出）
                        crate::event::emit(crate::event::Event::Window(
                            crate::event::window::Event::close_video_export_dialog(),
                        ));
                    }
                    V::ForceFinish => {
                        let st = &root.state.video_export_dialog;
                        root.state.video_export_dialog.overlay =
                            VideoExportOverlayState::Completed {
                                total_frames: st.total_frames,
                                elapsed_secs: 0.0,
                                avg_fps: st.render_fps,
                            };
                    }
                    V::DismissOverlay => {
                        root.state.video_export_dialog.overlay = VideoExportOverlayState::None;
                        root.state.video_export_dialog.preview_frame = None;
                        // 在对话框窗口中关闭窗口
                        if root.state.is_dialog_window {
                            root.state.dialog_result = Some(DialogResult::Cancel);
                        }
                    }
                    V::ContainerChanged(v) => {
                        root.state.video_export_dialog.container = v;
                    }
                    V::CodecChanged(v) => {
                        root.state.video_export_dialog.codec = v;
                    }
                    V::BackendChanged(v) => {
                        root.state.video_export_dialog.backend = v;
                    }
                    V::QualityChanged(v) => {
                        root.state.video_export_dialog.quality = v;
                    }
                    V::WidthChanged(v) => {
                        if v.chars().all(|c| c.is_ascii_digit())
                            && let Ok(w) = v.parse::<u32>()
                        {
                            root.state.video_export_dialog.width = w;
                        }
                    }
                    V::HeightChanged(v) => {
                        if v.chars().all(|c| c.is_ascii_digit())
                            && let Ok(h) = v.parse::<u32>()
                        {
                            root.state.video_export_dialog.height = h;
                        }
                    }
                    V::FpsChanged(v) => {
                        root.state.video_export_dialog.fps = v;
                    }
                    V::OutputPathChanged(v) => {
                        root.state.video_export_dialog.output_path = v;
                    }
                    V::BrowseOutput => {
                        let st = &root.state.video_export_dialog;
                        let ext = st.container.to_lowercase();
                        let default_name = format!("output.{}", ext);
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(&default_name)
                            .add_filter(&st.container, &[ext.as_str()])
                            .save_file()
                        {
                            root.state.video_export_dialog.output_path =
                                path.to_string_lossy().to_string();
                        }
                    }
                    V::UpdateProgress {
                        message,
                        progress,
                        current_frame,
                        total_frames,
                        fps,
                    } => {
                        let st = &mut root.state.video_export_dialog;
                        st.status_message = message;
                        st.progress = progress;
                        st.current_frame = current_frame;
                        st.total_frames = total_frames;
                        st.render_fps = fps;
                    }
                    V::ExportCompleted => {
                        // 由 Runner 回调设置具体字段，此处不处理
                    }
                    V::ExportFailed(err) => {
                        root.state.video_export_dialog.overlay =
                            VideoExportOverlayState::Error(err);
                    }
                    V::UpdatePreviewFrame { .. } => {
                        // 由 Host 直接处理，此处不需要
                    }
                }
                None
            }
            // 音符变速对话框消息
            Message::SpeedChange(action) => {
                use SpeedChangeAction as S;
                match action {
                    S::OpenDialog => {
                        root.state.speed_change_dialog.is_open = true;
                    }
                    S::CloseDialog => {
                        root.state.speed_change_dialog.is_open = false;
                        root.state.dialog_result = Some(DialogResult::Cancel);
                    }
                    S::Confirm => {
                        if let Some(factor) = root.state.speed_change_dialog.parse_factor() {
                            root.toolbar.speed_factor = factor;
                            tracing::info!("Root: 速度因子已更新为 {}", factor);
                            root.state.dialog_result = Some(DialogResult::SpeedChange { factor });
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
                    }
                    S::FactorChanged(value) => {
                        root.state.speed_change_dialog.factor_input = value;
                    }
                }
                None
            }

            other => Some(other),
        }
    }
}
