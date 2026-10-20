//! 音频导出窗口事件处理

use crate::runner::RunnerInner;
use std::path::PathBuf;
use std::sync::Arc;

use lumino_core::{tr, trf};
use lumino_export::audio::codec::AudioCodec;
use lumino_export::audio::config::{
    AudioBackendKind, AudioChannelMode, AudioInterpolation, AudioRenderConfig, ThreadMode,
    layer_limit_from_ui,
};
use lumino_export::audio::control::AudioExportControl;

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

        // DEBT-07 #124：导出可重入守卫——已有音频导出（控制句柄存在）或视频导出
        // 进行中时拒绝，避免进度通道/控制句柄被覆盖、两线程并发写盘。
        if self.window_state.audio_export_control.is_some()
            || self
                .window_state
                .video_export_running
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            tracing::warn!("已有导出任务进行中，忽略新的音频导出请求");
            self.window_state
                .window
                .ui_mut()
                .set_status_message(Some(tr!("已有导出任务进行中，请等待完成").to_string()));
            return;
        }

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

        // 1.1 创建导出控制句柄（暂停/中止）
        let control = Arc::new(AudioExportControl::new());
        self.window_state.audio_export_control = Some(Arc::clone(&control));

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
            layer_limit: layer_limit_from_ui(layer_limit),
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
            // PREF-002：CPU 块式渲染块大小（默认 256 帧；0/1 = 逐事件精确模式）
            block_frames: 256,
            // REND-002：多端口由渲染入口按文档/文件推导（此处不感知文档，保持 0）。
            midi_max_port: 0,
            backend: backend_kind,
            progress_callback: Some(progress_cb),
            control: Some(Arc::clone(&control)),
        };

        // 2. tempo 无需覆盖（2026-10-20 审计 §三）：`EditorData` 的 tempo 写入入口
        //    （set_tempo_points / set_tempo_point / add / remove）都会经
        //    `sync_tempo_to_document` 写回 `document.tempo_changes`，故文档快照里
        //    已是编辑后的值。原 `editor_tempos` 覆盖逻辑已删除。

        // 3. 在后台线程执行音频渲染，避免阻塞主线程 UI
        let output_path_display = config.output_path.display().to_string();
        let doc_clone = document.clone();
        let progress_tx_final = progress_tx.clone();
        let backend_for_log = config.backend;
        // 立即发送初始进度，避免 UI 长时间停在 0% 无反馈（GPU 初始化可能耗时）
        let _ = progress_tx.send((
            trf!("正在初始化 {backend} 渲染...", backend = backend_for_log),
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
                        let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
                        let total_ctrl = doc.control_events.len();
                        tracing::info!(
                            "内存文档快照: tracks={}, notes={}, ctrls={}, division={}, tempo_changes={}",
                            doc.track_count,
                            total_notes,
                            total_ctrl,
                            doc.division,
                            doc.tempo_changes.len()
                        );
                        // 空文档回退到文件模式（macOS 上文档可能为空，避免直接报错）
                        if total_notes == 0 && total_ctrl == 0 {
                            tracing::warn!(
                                "内存文档为空（0 notes），回退到文件模式: {:?}",
                                config.midi_path
                            );
                            if config.midi_path.exists() {
                                lumino_export::audio::render_audio(&config)
                            } else {
                                Err(lumino_export::ExportError::AudioWrite(
                                    tr!("内存文档为空且 MIDI 文件不存在，请先加载 MIDI 或检查 MIDI 路径")
                                        .to_string(),
                                ))
                            }
                        } else {
                            // Arc 快照不可变：克隆 owned 副本。
                            // ChunkedList 为块级浅拷贝（O(块数) 指针拷贝），不复制音符数据。
                            // tempo 已在文档内同步（审计 §三），无需再覆盖。
                            let snapshot = (**doc).clone();
                            lumino_export::audio::render_audio_from_document(&config, &snapshot)
                        }
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
                            tr!("导出完成").to_string(),
                            1.0,
                            0,
                            0.0,
                            elapsed.as_secs_f64(),
                        ));
                    }
                    Err(e) => {
                        // 中止为可预期操作，删除残留文件后以“已中止”通知 UI
                        if matches!(e, lumino_export::ExportError::Aborted) {
                            let _ = std::fs::remove_file(&output_path_display);
                            tracing::info!("音频导出已中止，已清理: {}", output_path_display);
                            let _ = progress_tx_final.send((
                                tr!("已中止").to_string(),
                                -1.0,
                                0,
                                0.0,
                                0.0,
                            ));
                        } else {
                            tracing::error!("音频导出失败: {e}");
                            let _ = progress_tx_final
                                .send((format!("{e}"), -1.0, 0, 0.0, 0.0));
                        }
                    }
                }
            });
    }
}
