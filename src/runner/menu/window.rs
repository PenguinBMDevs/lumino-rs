//! Runner 窗口事件处理

use std::sync::atomic::{AtomicBool, Ordering};

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_export::audio::config::{
    AudioChannelMode, AudioInterpolation, AudioRenderConfig, ProgressCallback, ThreadMode,
};
use lumino_ui::event::window::Event as WindowEvent;
use std::path::PathBuf;
use std::sync::Arc;

mod video_export;

impl RunnerInner {
    /// 处理窗口事件
    pub(super) fn handle_window_event(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::Dialog(e) => self.handle_dialog_events(e),
            WindowEvent::Collaboration(e) => self.handle_collaboration_events(e),
            WindowEvent::Sync(e) => self.handle_sync_events(e),
            _ => {}
        }
    }

    fn handle_dialog_events(&mut self, window_event: lumino_ui::event::window::dialog::Event) {
        use lumino_ui::event::window::dialog::Event::*;
        match window_event {
            OpenCustomPrecisionDialog => {
                tracing::info!("请求打开自定义精度对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::CustomPrecision);
            }
            CloseCustomPrecisionDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::CustomPrecision);
                tracing::info!("请求关闭自定义精度对话框");
            }
            ApplyCustomPrecision(_, _) => {
                // 应用精度（在对话框结果中处理）
            }
            OpenCollaborationDialog => {
                tracing::info!("请求打开协作对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::Collaboration);
            }
            CloseCollaborationDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("请求关闭协作对话框");
            }
            OpenProjectSettingsDialog => {
                tracing::info!("请求打开工程设置对话框");
                // 优先使用已保存的项目标题，回退到文件名
                let saved_title = self.window_state.window.ui().get_project_settings_title();
                let display_title = if saved_title.is_empty() {
                    self.midi_state
                        .current_midi_source
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "无标题".to_string())
                } else {
                    saved_title
                };
                let title = format!("{} - Lumino Midi", display_title);
                self.window_state
                    .dialog_manager
                    .open_project_settings(title);
            }
            CloseProjectSettingsDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::ProjectSettings);
                tracing::info!("请求关闭工程设置对话框");
            }
            ApplyProjectSettings {
                title,
                tempo,
                copyright,
            } => {
                tracing::info!(
                    "应用工程设置: 标题={}, BPM={}, 版权={}",
                    title,
                    tempo,
                    copyright
                );
                // 应用设置到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_project_settings(title, tempo, copyright);
            }
            OpenSpeedChangeDialog => {
                tracing::info!("请求打开音符变速对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::SpeedChange);
            }
            OpenVideoExportDialog => {
                tracing::info!("请求打开视频导出对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::VideoExport);
            }
            CloseVideoExportDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::VideoExport);
                // 设置取消标志，让后台 video-render 线程退出
                self.window_state
                    .video_export_cancel
                    .store(true, Ordering::Relaxed);
            }
            CloseSpeedChangeDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::SpeedChange);
                tracing::info!("请求关闭音符变速对话框");
            }
            ConfirmSpeedChange(factor) => {
                tracing::info!("应用音符变速: 倍率={}", factor);
                // 应用变速到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_speed_change(factor);
            }
            OpenLoadConfirmDialog { .. } => {}
            StartAudioExport {
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
                document,
            } => {
                use std::time::Instant;

                // 根据是否有内存中的 MidiDocument 选择渲染模式
                let mode_str = if document.is_some() {
                    "内存模式（零拷贝）"
                } else {
                    "文件模式"
                };
                tracing::info!("开始音频导出 [{mode_str}]: MIDI={midi_path}, SF2={soundfont_path}");

                let midi_path = PathBuf::from(&midi_path);
                let soundfont_path = PathBuf::from(&soundfont_path);
                let output_path = PathBuf::from(&output_path);

                // 解析枚举值（来自 UI 的 Debug 格式）
                let channel_mode = AudioChannelMode::from_str(&channels).unwrap_or_default();
                let interpolation_val =
                    AudioInterpolation::from_str(&interpolation).unwrap_or_default();
                let channel_threading_val =
                    ThreadMode::from_str(&channel_threading).unwrap_or_default();
                let key_threading_val = ThreadMode::from_str(&key_threading).unwrap_or_default();

                let config = AudioRenderConfig {
                    sample_rate: sample_rate.max(8000),
                    channels: channel_mode,
                    layer_limit: layer_limit.max(1),
                    channel_threading: channel_threading_val,
                    key_threading: key_threading_val,
                    interpolation: interpolation_val,
                    apply_limiter,
                    disable_fade_out,
                    linear_envelope,
                };

                let progress_cb: ProgressCallback = Box::new(move |current, total, label| {
                    // 通过进度通道发送到 UI 线程
                    tracing::debug!("音频导出进度: {}/{} ({})", current, total, label);
                });

                let start = Instant::now();
                match lumino_export::audio::render_audio_to_file(
                    &midi_path,
                    &soundfont_path,
                    &output_path,
                    &config,
                    document,
                    progress_cb,
                ) {
                    Ok(_) => {
                        let elapsed = start.elapsed();
                        tracing::info!(
                            "音频导出完成: 耗时 {:.1}s, 输出={}",
                            elapsed.as_secs_f64(),
                            output_path.display()
                        );
                        let main_ui = self.window_state.window.ui_mut();
                        // 使用音频导出对话框的完成回调
                        main_ui.audio_export_completed(output_path.to_string_lossy().to_string());
                    }
                    Err(e) => {
                        tracing::error!("音频导出失败: {e}");
                        let main_ui = self.window_state.window.ui_mut();
                        main_ui.set_audio_export_failed(format!("音频导出失败: {e}"));
                    }
                }
            }
            StartVideoExport {
                width,
                height,
                fps,
                ppq,
                key_count,
                container,
                codec,
                backend,
                quality,
                output_path,
                document,
            } => {
                use lumino_export::video::{
                    FfmpegEncoder, VideoExportConfig,
                    config::{Container, EncoderBackend, QualityPreset, VideoCodec},
                };
                use lumino_gfx::render_thread::{ControlCommand, FrameSender, RenderCommand};
                use std::str::FromStr;

                tracing::info!(
                    "开始视频导出: {}x{} @ {}fps, 容器={:?}, 编解码器={:?}",
                    width,
                    height,
                    fps,
                    container,
                    codec
                );

                // 打开视频导出对话框（进度显示）
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::VideoExport);

                // 解析配置枚举
                let container = Container::from_str(&container).unwrap_or(Container::Mp4);
                let codec = VideoCodec::from_str(&codec).unwrap_or(VideoCodec::H264);
                let backend =
                    EncoderBackend::from_str(&backend).unwrap_or(EncoderBackend::Software);
                let quality = QualityPreset::from_str(&quality).unwrap_or_default();

                // 获取渲染线程命令发送端
                let main_ui = self.window_state.window.ui();
                let cmd_sender = main_ui.render_command_sender();

                if cmd_sender.is_none() {
                    tracing::error!("视频导出失败：渲染线程未启动");
                    let main_ui = self.window_state.window.ui_mut();
                    main_ui.set_video_export_failed("渲染线程未启动".to_string());
                    return;
                }

                let cmd_sender = cmd_sender.expect("已检查");

                // 创建进度通道（复用音频导出的进度通道机制）
                let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
                self.window_state.export_progress_rx = Some(progress_rx);

                // 创建预览帧通道
                let (preview_tx, preview_rx) = tokio::sync::mpsc::unbounded_channel();
                self.window_state.video_preview_rx = Some(preview_rx);

                // 构建 VideoExportConfig
                let config = VideoExportConfig {
                    width,
                    height,
                    fps: fps as f64,
                    container: container.clone(),
                    codec,
                    backend,
                    output_path: std::path::PathBuf::from(&output_path),
                    quality,
                };

                // 获取 MidiDocument（编辑器模式）
                let document = match document {
                    Some(doc) => doc,
                    None => {
                        tracing::error!("视频导出失败：无 MidiDocument（暂不支持流式模式）");
                        let _ = progress_tx.send(("导出失败：无 MIDI 数据".to_string(), -1.0));
                        return;
                    }
                };

                let ppq = ppq.max(1) as u32;
                let fps_f64 = fps as f64;

                // 创建取消标志
                let cancel_flag = Arc::new(AtomicBool::new(false));
                self.window_state.video_export_cancel = cancel_flag.clone();

                // 后台线程：逐帧渲染 + FFmpeg 编码
                std::thread::Builder::new()
                    .name("video-render".into())
                    .spawn(move || {
                        let start = std::time::Instant::now();

                        // 创建 FFmpeg 编码器
                        let mut encoder = match FfmpegEncoder::new(&config) {
                            Ok(e) => e,
                            Err(e) => {
                                tracing::error!("FFmpeg 创建失败: {e}");
                                let _ = progress_tx.send((format!("导出失败: {e}"), -1.0));
                                return;
                            }
                        };

                        // 创建帧数据通道
                        let (frame_tx, frame_rx) = std::sync::mpsc::channel::<Vec<u8>>();

                        // 发送 StartVideoExport 命令
                        if cmd_sender
                            .send(RenderCommand::Control(ControlCommand::StartVideoExport {
                                width,
                                height,
                                frame_tx: FrameSender(frame_tx),
                            }))
                            .is_err()
                        {
                            tracing::error!("发送 StartVideoExport 命令失败");
                            let _ =
                                progress_tx.send(("导出失败：渲染线程通信错误".to_string(), -1.0));
                            return;
                        }

                        // 计算总帧数
                        let tempo_changes = &document.tempo_changes;
                        let total_ticks = document.total_ticks;
                        let duration_secs =
                            video_export::compute_duration_secs(tempo_changes, total_ticks, ppq);
                        let total_frames = config.total_frames(duration_secs);

                        tracing::info!(
                            "视频导出: 总时长 {:.1}s, 总帧数 {}, PPQ {}",
                            duration_secs,
                            total_frames,
                            ppq
                        );

                        let mut last_stat_time = std::time::Instant::now();
                        let mut frames_since_stat = 0u64;
                        let mut smoothed_fps = 0.0f64;
                        let mut last_preview_time = std::time::Instant::now();

                        // ★ 生成键盘贴图（使用 CPU 贴图方式，在帧数据上合成）
                        let (keyboard_pixels, kb_w, kb_h) =
                            video_export::generate_keyboard_texture(width, height, key_count);

                        // 逐帧渲染
                        let mut cancelled = false;
                        for frame in 0..total_frames {
                            // 检查取消标志
                            if cancel_flag.load(Ordering::Relaxed) {
                                tracing::info!("视频导出：用户取消，正在收尾...");
                                cancelled = true;
                                break;
                            }
                            let time_sec = frame as f64 / fps_f64;
                            let tick = video_export::seconds_to_tick(time_sec, tempo_changes, ppq);

                            // 计算 scroll_x / zoom_x，用于标尺小节号合成
                            let video_kb_width = 60.0f32;
                            let video_viewport_tick_span = (ppq * 16).max(1) as f32;
                            let video_zoom_x =
                                (width as f32 - video_kb_width) / video_viewport_tick_span;
                            let video_scroll_x = tick as f32 * video_zoom_x;

                            // 构建 RenderParams（简化版：默认值 + 视频分辨率 + scroll 跟随播放头）
                            let params = video_export::build_video_render_params(
                                width, height, tick, &document, ppq, key_count,
                            );

                            // 发送渲染命令
                            if cmd_sender
                                .send(RenderCommand::Control(ControlCommand::RenderVideoFrame(
                                    Box::new(params),
                                )))
                                .is_err()
                            {
                                tracing::error!("发送 RenderVideoFrame 命令失败");
                                let _ = progress_tx
                                    .send(("导出失败：渲染线程通信错误".to_string(), -1.0));
                                cancelled = true;
                                break;
                            }

                            // 接收帧数据（阻塞）
                            let mut frame_data = match frame_rx.recv() {
                                Ok(data) if !data.is_empty() => data,
                                Ok(_) => {
                                    tracing::warn!("帧 {} 读回为空，跳过", frame);
                                    continue;
                                }
                                Err(_) => {
                                    tracing::error!("帧数据通道关闭");
                                    let _ = progress_tx
                                        .send(("导出失败：帧数据通道关闭".to_string(), -1.0));
                                    cancelled = true;
                                    break;
                                }
                            };

                            // ★ 在帧上合成键盘贴图（in-place 修改，BGRA 格式）
                            if !keyboard_pixels.is_empty() {
                                video_export::composite_keyboard(
                                    &mut frame_data,
                                    width,
                                    height,
                                    &keyboard_pixels,
                                    kb_w,
                                    kb_h,
                                );
                            }

                            // ★ 在帧上合成标尺小节号（in-place 修改，BGRA 格式）
                            video_export::composite_ruler_numbers(
                                &mut frame_data,
                                width,
                                height,
                                video_scroll_x,
                                video_zoom_x,
                                video_kb_width,
                                ppq,
                            );

                            // 帧数据到达后再次检查取消（避免阻塞期间错过取消信号）
                            if cancel_flag.load(Ordering::Relaxed) {
                                tracing::info!("视频导出：帧数据到达后检测到取消，正在收尾...");
                                // 写入当前已收到的帧后再收尾
                                let _ = encoder.write_frame(frame_data);
                                cancelled = true;
                                break;
                            }

                            // 写入帧
                            if let Err(e) = encoder.write_frame(frame_data) {
                                tracing::error!("写入视频帧失败: {e}");
                                let _ = progress_tx.send((format!("导出失败: {e}"), -1.0));
                                cancelled = true;
                                break;
                            }

                            // 更新统计
                            frames_since_stat += 1;
                            let elapsed = last_stat_time.elapsed();
                            if elapsed >= std::time::Duration::from_secs(2) {
                                let fps = frames_since_stat as f64 / elapsed.as_secs_f64();
                                smoothed_fps = if smoothed_fps == 0.0 {
                                    fps
                                } else {
                                    smoothed_fps * 0.7 + fps * 0.3
                                };
                                let progress = (frame + 1) as f64 / total_frames as f64;
                                let eta_secs = (total_frames - frame - 1) as f64 / smoothed_fps;
                                tracing::info!(
                                    "视频导出: 帧 {}/{} ({:.0}%), FPS={:.0}, ETA={:.0}s",
                                    frame + 1,
                                    total_frames,
                                    progress * 100.0,
                                    smoothed_fps,
                                    eta_secs
                                );
                                let _ = progress_tx.send((
                                    format!(
                                        "{:.0}% | FPS {:.0} | ETA {:.0}s",
                                        progress * 100.0,
                                        smoothed_fps,
                                        eta_secs
                                    ),
                                    progress,
                                ));
                                last_stat_time = std::time::Instant::now();
                                frames_since_stat = 0;
                            }

                            // 预览帧更新：约 5fps
                            if last_preview_time.elapsed() >= std::time::Duration::from_millis(200)
                            {
                                let _ = preview_tx.send(());
                                last_preview_time = std::time::Instant::now();
                            }
                        }

                        // 完成编码
                        if !cancelled {
                            let elapsed = start.elapsed();
                            tracing::info!("视频导出完成: 耗时 {:.1}s", elapsed.as_secs_f64());
                            let _ = progress_tx.send(("导出完成".to_string(), 1.0));
                            let _ = encoder.finish();
                        }
                    });
            }
            SaveVideoExportConfig { .. } => {}
            OpenAudioExportDialog => {
                tracing::info!("请求打开音频导出对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::AudioExport);
            }
            CloseAudioExportDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::AudioExport);
                tracing::info!("请求关闭音频导出对话框");
            }
            SaveAudioExportConfig { .. } => {}
            ChangeAudioSampleRate(sample_rate) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_sample_rate(sample_rate);
            }
            ChangeAudioChannels(channels) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_channels(channels);
            }
            ChangeAudioLayerLimit(layer_limit) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_layer_limit(layer_limit);
            }
            ChangeAudioChannelThreading(threading) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_channel_threading(threading);
            }
            ChangeAudioKeyThreading(threading) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_key_threading(threading);
            }
            ChangeAudioInterpolation(interpolation) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_interpolation(interpolation);
            }
            ChangeAudioLimiter(enabled) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_limiter(enabled);
            }
            ChangeAudioFadeOut(disable_fade_out) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_fade_out(disable_fade_out);
            }
            ChangeAudioLinearEnvelope(linear_envelope) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.change_audio_linear_envelope(linear_envelope);
            }
            _ => {
                tracing::warn!("未处理的窗口事件: {:?}", window_event);
            }
        }
    }

    fn handle_collaboration_events(
        &mut self,
        window_event: lumino_ui::event::window::collaboration::Event,
    ) {
        use lumino_ui::event::window::collaboration::Event::*;
        match window_event {
            StartCollaboration(doc_id) => {
                tracing::info!("请求启动协作: doc_id={}", doc_id);
                self.handle_collaboration_start(doc_id);
            }
            StopCollaboration => {
                tracing::info!("请求停止协作");
                self.handle_collaboration_stop();
            }
            JoinCollaboration(doc_id) => {
                tracing::info!("请求加入协作: doc_id={}", doc_id);
                self.handle_collaboration_join(doc_id);
            }
            LeaveCollaboration => {
                tracing::info!("请求离开协作");
                self.handle_collaboration_leave();
            }
            RemoteCursorChanged { .. } => {}
            _ => {
                tracing::warn!("未处理的协作事件: {:?}", window_event);
            }
        }
    }

    fn handle_sync_events(&mut self, window_event: lumino_ui::event::window::sync::Event) {
        use lumino_ui::event::window::sync::Event::*;
        match window_event {
            RemoteNoteAdded {
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_remote_note_added(tick, key, length, velocity, channel, track_index);
            }
            RemoteNoteDeleted {
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_remote_note_deleted(tick, key, length, velocity, channel, track_index);
            }
            RemoteNoteMoved {
                from_tick,
                to_tick,
                from_key,
                to_key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_remote_note_moved(
                    from_tick,
                    to_tick,
                    from_key,
                    to_key,
                    length,
                    velocity,
                    channel,
                    track_index,
                );
            }
            RemoteTrackAdded { track_index } => {
                self.handle_remote_track_added(track_index);
            }
            RemoteTrackRemoved { track_index } => {
                self.handle_remote_track_removed(track_index);
            }
            LocalNoteAdded {
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_local_note_added(tick, key, length, velocity, channel, track_index);
            }
            LocalNoteDeleted {
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_local_note_deleted(tick, key, length, velocity, channel, track_index);
            }
            LocalTrackAdded { track_index } => {
                self.handle_local_track_added(track_index);
            }
        }
    }
}