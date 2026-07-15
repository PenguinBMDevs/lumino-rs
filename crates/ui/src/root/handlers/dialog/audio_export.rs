//! 音频导出面板处理器

use crate::message::{AudioExportAction, Message};
use crate::root::Root;
use crate::util::{parse_u8_bounded, parse_uint};

use super::DialogHandler;

/// 将音频导出渲染状态重置为“初始化中”。
///
/// `Confirm` 与 `StartRendering` 两处原本各有一份逐字段赋值的 5 行样板，
/// 新增字段时极易只改一处导致状态不一致，故抽离为统一入口。
fn begin_audio_export_render(root: &mut Root) {
    let st = &mut root.state.audio_export_dialog;
    st.is_rendering = true;
    st.render_completed = false;
    st.render_error = None;
    st.render_progress = 0.0;
    st.render_message = "正在初始化...".to_string();
}

/// 根据音频导出对话框状态构建导出配置
fn build_audio_export_config(
    st: &crate::state::root_state::AudioExportDialogState,
) -> crate::event::window::dialog::AudioExportConfig {
    crate::event::window::dialog::AudioExportConfig {
        midi_path: st.midi_path.clone(),
        soundfont_path: st.soundfont_path.clone(),
        output_path: st.output_path.clone(),
        sample_rate: st.sample_rate,
        channels: st.channels,
        layer_limit: st.layers,
        channel_threading: st.channel_threading,
        key_threading: st.key_threading,
        interpolation: st.interpolation,
        apply_limiter: st.apply_limiter,
        disable_fade_out: st.disable_fade_out,
        linear_envelope: st.linear_envelope,
        audio_format: st.format,
        audio_bitrate: st.audio_bitrate,
        ignore_program_changes: st.ignore_program_changes,
        filter_velocity: st.filter_velocity,
        velocity_low: st.velocity_low,
        velocity_high: st.velocity_high,
        filter_key: st.filter_key,
        key_low: st.key_low,
        key_high: st.key_high,
        note_force_end_delay: st.note_force_end_delay,
    }
}

impl DialogHandler {
    pub(super) fn handle_audio_export(
        &self,
        root: &mut Root,
        action: AudioExportAction,
    ) -> Option<Message> {
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
                begin_audio_export_render(root);

                // 从 dialog state 读取配置，发送事件到 runner
                let st = &root.state.audio_export_dialog;

                // 检查内存中是否有 MidiDocument
                let document = root.midi.document.as_ref().map(|doc| {
                    tracing::info!("使用内存中的 MidiDocument 进行音频导出（零拷贝模式）");
                    std::sync::Arc::clone(doc)
                });

                if document.is_none() {
                    tracing::info!("内存中没有 MidiDocument，使用文件模式: {:?}", st.midi_path);
                }

                let config = build_audio_export_config(st);
                let ev = crate::event::window::Event::start_audio_export(config, document);
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
            A::BitrateChanged(value) => {
                if let Some(v) = parse_uint(&value) {
                    root.state.audio_export_dialog.audio_bitrate = v;
                }
            }
            A::SampleRateChanged(value) => {
                root.state.audio_export_dialog.sample_rate = value;
            }
            A::ChannelsChanged(value) => {
                root.state.audio_export_dialog.channels = value;
            }
            A::LayersChanged(value) => {
                if let Some(v) = parse_uint(&value) {
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
            A::IgnoreProgramChangesChanged(value) => {
                root.state.audio_export_dialog.ignore_program_changes = value;
            }
            A::FilterVelocityChanged(value) => {
                root.state.audio_export_dialog.filter_velocity = value;
            }
            A::VelocityLowChanged(value) => {
                if let Some(v) = parse_u8_bounded(&value, 127) {
                    root.state.audio_export_dialog.velocity_low = v;
                }
            }
            A::VelocityHighChanged(value) => {
                if let Some(v) = parse_u8_bounded(&value, 127) {
                    root.state.audio_export_dialog.velocity_high = v;
                }
            }
            A::FilterKeyChanged(value) => {
                root.state.audio_export_dialog.filter_key = value;
            }
            A::KeyLowChanged(value) => {
                if let Some(v) = parse_u8_bounded(&value, 127) {
                    root.state.audio_export_dialog.key_low = v;
                }
            }
            A::KeyHighChanged(value) => {
                if let Some(v) = parse_u8_bounded(&value, 127) {
                    root.state.audio_export_dialog.key_high = v;
                }
            }
            A::NoteForceEndDelayChanged(value) => {
                if let Some(v) = parse_uint(&value) {
                    root.state.audio_export_dialog.note_force_end_delay = v;
                }
            }
            A::BrowseOutput => {
                let st = &root.state.audio_export_dialog;
                let current = st.output_path.clone();
                let ext = st.format.extension();
                let default_name = std::path::Path::new(&current)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("export.wav");
                let default_dir = std::path::Path::new(&current)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or(".");
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(
                        default_name
                            .rsplit_once('.')
                            .map(|(base, _)| format!("{base}.{ext}"))
                            .unwrap_or_else(|| format!("export.{ext}")),
                    )
                    .set_directory(default_dir)
                    .add_filter(&format!("{} 文件", st.format), &[ext])
                    .save_file()
                {
                    root.state.audio_export_dialog.output_path = path.to_string_lossy().to_string();
                }
            }
            A::BrowseMidi => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("音乐文件", &["mid", "midi", "lmpj"])
                    .add_filter("MIDI 文件", &["mid", "midi"])
                    .add_filter("Lumino 项目", &["lmpj"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                {
                    root.state.audio_export_dialog.midi_path = path.to_string_lossy().to_string();
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
                begin_audio_export_render(root);
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
                root.state.audio_export_dialog.render_message = format!("导出失败: {error}");
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
}
