//! 音频导出窗口事件处理

use crate::runner::RunnerInner;
use std::path::PathBuf;
use std::sync::Arc;

use lumino_export::audio::codec::AudioCodec;
use lumino_export::audio::config::{
    AudioBackendKind, AudioChannelMode, AudioInterpolation, AudioRenderConfig, ThreadMode,
};

impl RunnerInner {
    // clippy::boxed_local: Box 来自 `dialog::Event::StartAudioExport` 枚举解包，
    // 用于 UI→Runner 消息零拷贝传递（见 Message 枚举体积守卫测试）。
    #[allow(clippy::boxed_local)]
    pub(crate) fn handle_start_audio_export(
        &mut self,
        config: Box<lumino_message::events::window::dialog::AudioExportConfig>,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) {
        use std::time::Instant;

        let lumino_message::events::window::dialog::AudioExportConfig {
            midi_path,
            soundfont_path,
            output_path,
            sample_rate,
            channels,
            layer_limit,
            channel_threading,
            key_threading,
            interpolation,
            apply_limiter,
            disable_fade_out,
            linear_envelope,
            audio_format,
            audio_bitrate,
            ignore_program_changes,
            filter_velocity,
            velocity_low,
            velocity_high,
            filter_key,
            key_low,
            key_high,
            note_force_end_delay,
            backend,
        } = *config;

        // 根据是否有内存中的 MidiDocument 选择渲染模式
        let mode_str = if document.is_some() {
            "内存模式（零拷贝）"
        } else {
            "文件模式"
        };
        tracing::info!("开始音频导出 [{mode_str}]: MIDI={midi_path}, SF2={soundfont_path}");

        let midi_path_buf = PathBuf::from(&midi_path);
        let output_path_buf = PathBuf::from(&output_path);

        let channel_mode = match channels {
            lumino_message::events::window::audio::AudioChannels::Mono => AudioChannelMode::Mono,
            lumino_message::events::window::audio::AudioChannels::Stereo => {
                AudioChannelMode::Stereo
            }
        };
        let interpolation_val = match interpolation {
            lumino_message::events::window::audio::Interpolation::None => {
                AudioInterpolation::Nearest
            }
            lumino_message::events::window::audio::Interpolation::Linear => {
                AudioInterpolation::Linear
            }
        };
        let channel_threading_val = match channel_threading {
            lumino_message::events::window::audio::ThreadingOption::None => ThreadMode::None,
            lumino_message::events::window::audio::ThreadingOption::Auto => ThreadMode::Auto,
            lumino_message::events::window::audio::ThreadingOption::Manual(n) => {
                ThreadMode::Manual(n)
            }
        };
        let key_threading_val = match key_threading {
            lumino_message::events::window::audio::ThreadingOption::None => ThreadMode::None,
            lumino_message::events::window::audio::ThreadingOption::Auto => ThreadMode::Auto,
            lumino_message::events::window::audio::ThreadingOption::Manual(n) => {
                ThreadMode::Manual(n)
            }
        };

        let audio_codec = match audio_format {
            lumino_message::events::window::audio::AudioFormat::WAV => AudioCodec::Pcm,
            lumino_message::events::window::audio::AudioFormat::FLAC => AudioCodec::Flac,
            lumino_message::events::window::audio::AudioFormat::MP3 => AudioCodec::Mp3,
            lumino_message::events::window::audio::AudioFormat::Ogg => AudioCodec::Vorbis,
            lumino_message::events::window::audio::AudioFormat::WavPack => AudioCodec::WavPack,
        };

        let backend_kind = match backend {
            lumino_message::events::window::audio::AudioBackend::Cpu => AudioBackendKind::Cpu,
            lumino_message::events::window::audio::AudioBackend::Gpu => AudioBackendKind::Gpu,
        };

        // 1. 创建进度通道，将渲染进度发回主线程更新 UI
        let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
        self.window_state.export_progress_rx = Some(progress_rx);

        let progress_tx_cb = progress_tx.clone();
        let progress_cb: lumino_export::audio::config::ProgressCallback =
            Arc::new(move |msg: String, pct: f64| {
                let _ = progress_tx_cb.send((msg, pct, 0, 0.0, 0.0));
            });

        let config = AudioRenderConfig {
            midi_path: midi_path_buf,
            soundfonts: vec![PathBuf::from(&soundfont_path)],
            output_path: output_path_buf,
            sample_rate: sample_rate.max(8000),
            channels: channel_mode,
            layer_limit: Some(layer_limit.max(1) as usize),
            channel_threading: channel_threading_val,
            key_threading: key_threading_val,
            interpolation: interpolation_val,
            apply_limiter,
            disable_fade_out,
            linear_envelope,
            audio_codec,
            audio_bitrate,
            ignore_program_changes,
            filter_velocity,
            velocity_low,
            velocity_high,
            filter_key,
            key_low,
            key_high,
            note_force_end_delay,
            backend: backend_kind,
            progress_callback: Some(progress_cb),
        };

        // 2. 用编辑器 tempo_points 覆盖 doc 的加载时原始 tempo（与保存/云存/工程导出一致）：
        //    doc.tempo_changes 是加载文件时的值，用户经工程设置/速度面板修改的 BPM
        //    只写入 tempo_points、不回写 doc——不覆盖会按旧 tempo 导出（BPM 修改不生效）。
        let editor_tempos: Vec<(u32, f32)> = self
            .window_state
            .window
            .ui()
            .root()
            .editor
            .editor_state
            .data
            .tempo_points
            .iter()
            .map(|tp| (tp.tick.max(0.0) as u32, tp.bpm as f32))
            .collect();

        // 3. 在后台线程执行音频渲染，避免阻塞主线程 UI
        let output_path_display = config.output_path.display().to_string();
        let doc_clone = document.clone();
        let progress_tx_final = progress_tx.clone();
        let backend_for_log = config.backend;
        // 立即发送初始进度，避免 UI 长时间停在 0% 无反馈（GPU 初始化可能耗时）
        let _ = progress_tx.send((
            format!("正在初始化 {} 渲染...", backend_for_log),
            0.0,
            0,
            0.0,
            0.0,
        ));
        let _ = std::thread::Builder::new()
            .name("audio-render".into())
            .spawn(move || {
                let start = Instant::now();
                let render_result = match &doc_clone {
                    Some(doc) => {
                        // Arc 快照不可变：克隆 owned 副本后覆盖 tempo。
                        // ChunkedList 为块级浅拷贝（O(块数) 指针拷贝），不复制音符数据。
                        let mut snapshot = (**doc).clone();
                        if !editor_tempos.is_empty() {
                            snapshot.tempo_changes = editor_tempos;
                        }
                        lumino_export::audio::render_audio_from_document(&config, &snapshot)
                    }
                    None => lumino_export::audio::render_audio(&config),
                };

                match render_result {
                    Ok(_) => {
                        let elapsed = start.elapsed();
                        tracing::info!(
                            "音频导出完成: 耗时 {:.1}s, 输出={}",
                            elapsed.as_secs_f64(),
                            output_path_display,
                        );
                        let _ = progress_tx_final.send((
                            "导出完成".to_string(),
                            1.0,
                            0,
                            0.0,
                            elapsed.as_secs_f64(),
                        ));
                    }
                    Err(e) => {
                        tracing::error!("音频导出失败: {e}");
                        let _ = progress_tx_final.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
                    }
                }
            });
    }
}
