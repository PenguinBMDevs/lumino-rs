//! Runner 窗口事件处理

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_export::audio::config::{
    AudioChannelMode, AudioInterpolation, AudioRenderConfig, ProgressCallback, ThreadMode,
};
use lumino_ui::event::window::Event as WindowEvent;
use std::path::PathBuf;
use std::sync::Arc;

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
                let ch = match channels.as_str() {
                    "Mono" => AudioChannelMode::Mono,
                    _ => AudioChannelMode::Stereo,
                };
                let ct = match channel_threading.as_str() {
                    "None" => ThreadMode::None,
                    "Auto" => ThreadMode::Auto,
                    s if s.starts_with("Manual") => {
                        // format: "Manual(N)"
                        let n = s
                            .trim_start_matches("Manual(")
                            .trim_end_matches(')')
                            .parse::<u32>()
                            .unwrap_or(2);
                        ThreadMode::Manual(n)
                    }
                    _ => ThreadMode::Auto,
                };
                let kt = match key_threading.as_str() {
                    "None" => ThreadMode::None,
                    "Auto" => ThreadMode::Auto,
                    s if s.starts_with("Manual") => {
                        let n = s
                            .trim_start_matches("Manual(")
                            .trim_end_matches(')')
                            .parse::<u32>()
                            .unwrap_or(2);
                        ThreadMode::Manual(n)
                    }
                    _ => ThreadMode::Auto,
                };
                let ip = match interpolation.as_str() {
                    "None" => AudioInterpolation::Nearest,
                    _ => AudioInterpolation::Linear,
                };
                let layer_limit = if layer_limit == 0 {
                    None
                } else {
                    Some(layer_limit as usize)
                };

                // 创建进度通道（直接更新 audio_export_dialog 内嵌进度条）
                let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
                self.window_state.export_progress_rx = Some(progress_rx);

                // 创建进度回调
                let progress_tx_cb = progress_tx.clone();
                let progress_callback: ProgressCallback =
                    Arc::new(move |msg: String, progress: f64| {
                        let _ = progress_tx_cb.send((msg, progress));
                    });

                let config = AudioRenderConfig {
                    midi_path,
                    soundfonts: vec![soundfont_path],
                    output_path,
                    sample_rate,
                    channels: ch,
                    layer_limit,
                    channel_threading: ct,
                    key_threading: kt,
                    interpolation: ip,
                    apply_limiter,
                    disable_fade_out,
                    linear_envelope,
                    progress_callback: Some(progress_callback),
                };

                // 在后台线程执行渲染（避免阻塞 UI）
                std::thread::Builder::new()
                    .name("audio-render".into())
                    .spawn(move || {
                        let start = Instant::now();

                        // 根据是否有内存中的 MidiDocument 选择渲染方式
                        let render_result = if let Some(doc) = document {
                            tracing::info!("使用内存模式渲染音频（零拷贝）");
                            lumino_export::render_audio_from_document(&config, &doc)
                        } else {
                            tracing::info!("使用文件模式渲染音频: {:?}", config.midi_path);
                            lumino_export::render_audio(&config)
                        };

                        match render_result {
                            Ok(()) => {
                                let elapsed = start.elapsed();
                                tracing::info!(
                                    "音频导出完成: {:?}, 耗时 {:?}",
                                    config.output_path,
                                    elapsed
                                );
                                // 通知 UI 渲染完成
                                let _ = progress_tx.send(("导出完成".to_string(), 1.0));
                            }
                            Err(e) => {
                                tracing::error!("音频导出失败: {e}");
                                // 通知 UI 渲染失败
                                let _ = progress_tx.send((format!("导出失败: {e}"), -1.0));
                            }
                        }
                    })
                    .expect("无法创建音频渲染线程");
            }
            StartVideoExport {
                output_path,
                width,
                height,
                fps,
                container,
                codec,
                backend,
                quality,
                ppq,
                key_count,
                document,
            } => {
                use lumino_export::video::{
                    FfmpegEncoder, VideoExportConfig,
                    config::{Container, EncoderBackend, QualityPreset, VideoCodec},
                };
                use lumino_gfx::render_thread::{ControlCommand, FrameSender, RenderCommand};
                use std::str::FromStr;

                tracing::info!(
                    "开始视频导出: {}x{}@{}fps → {}",
                    width,
                    height,
                    fps,
                    output_path
                );

                // 打开视频导出对话框窗口
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
                        let duration_secs = compute_duration_secs(tempo_changes, total_ticks, ppq);
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

                        // 逐帧渲染
                        for frame in 0..total_frames {
                            let time_sec = frame as f64 / fps_f64;
                            let tick = seconds_to_tick(time_sec, tempo_changes, ppq);

                            // 构建 RenderParams（简化版：默认值 + 视频分辨率 + scroll 跟随播放头）
                            let params = build_video_render_params(
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
                                return;
                            }

                            // 接收帧数据（阻塞）
                            let frame_data = match frame_rx.recv() {
                                Ok(data) if !data.is_empty() => data,
                                Ok(_) => {
                                    tracing::warn!("帧 {} 读回为空，跳过", frame);
                                    continue;
                                }
                                Err(_) => {
                                    tracing::error!("帧数据通道关闭");
                                    let _ = progress_tx
                                        .send(("导出失败：帧数据通道关闭".to_string(), -1.0));
                                    return;
                                }
                            };

                            // 预览帧：每 500ms 发送一次，BGRA → RGBA 转换 + 降采样
                            // 必须在 encoder.write_frame(frame_data) 之前捕获数据
                            let pnow = std::time::Instant::now();
                            if pnow.duration_since(last_preview_time).as_millis() >= 500 {
                                last_preview_time = pnow;
                                let src_w = width as usize;
                                let src_h = height as usize;
                                let preview_w = src_w.min(320);
                                let preview_h = (src_h * preview_w / src_w).max(60);
                                let mut rgba = Vec::with_capacity(preview_w * preview_h * 4);
                                for py in 0..preview_h {
                                    let sy = py * src_h / preview_h;
                                    for px in 0..preview_w {
                                        let sx = px * src_w / preview_w;
                                        let src_idx = (sy * src_w + sx) * 4;
                                        rgba.push(frame_data[src_idx + 2]); // R
                                        rgba.push(frame_data[src_idx + 1]); // G
                                        rgba.push(frame_data[src_idx]); // B
                                        rgba.push(frame_data[src_idx + 3]); // A
                                    }
                                }
                                let _ = preview_tx.send((rgba, preview_w as u32, preview_h as u32));
                            }

                            // 写入 FFmpeg
                            if let Err(e) = encoder.write_frame(frame_data) {
                                tracing::error!("写入 FFmpeg 失败: {e}");
                                let _ = progress_tx.send((format!("导出失败: {e}"), -1.0));
                                return;
                            }

                            // FPS 统计
                            frames_since_stat += 1;
                            let now = std::time::Instant::now();
                            let stat_elapsed = now.duration_since(last_stat_time);
                            if stat_elapsed.as_millis() >= 500 {
                                let instant_fps =
                                    frames_since_stat as f64 / stat_elapsed.as_secs_f64();
                                smoothed_fps = smoothed_fps * 0.6 + instant_fps * 0.4;
                                frames_since_stat = 0;
                                last_stat_time = now;
                            }

                            // 进度回调
                            let progress = (frame + 1) as f64 / total_frames as f64;
                            let _ = progress_tx
                                .send((format!("帧 {}/{}", frame + 1, total_frames), progress));
                        }

                        // 发送 FinishVideoExport
                        let _ = cmd_sender
                            .send(RenderCommand::Control(ControlCommand::FinishVideoExport));

                        // 完成 FFmpeg 编码
                        match encoder.finish() {
                            Ok(()) => {
                                let elapsed = start.elapsed().as_secs_f64();
                                let avg_fps = if total_frames > 0 {
                                    total_frames as f64 / elapsed
                                } else {
                                    0.0
                                };
                                tracing::info!(
                                    "视频导出完成: {}帧, 耗时 {:.1}s, 平均 {:.1}fps",
                                    total_frames,
                                    elapsed,
                                    avg_fps
                                );
                                let _ = progress_tx.send(("导出完成".to_string(), 1.0));
                            }
                            Err(e) => {
                                tracing::error!("FFmpeg 编码完成失败: {e}");
                                let _ = progress_tx.send((format!("导出失败: {e}"), -1.0));
                            }
                        }
                    })
                    .expect("无法创建视频渲染线程");
            }
        }
    }

    fn handle_collaboration_events(
        &mut self,
        window_event: lumino_ui::event::window::collaboration::Event,
    ) {
        use lumino_ui::event::window::collaboration::Event::*;
        match window_event {
            Connect {
                host,
                port,
                username,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 连接到 {}:{}, 用户名: {}, 邀请码: {:?}",
                    host,
                    port,
                    username,
                    invite_code
                );
                let room_name = Some("Lumino 房间".to_string());
                self.handle_collaboration_connect(host, port, username, room_name, invite_code);
            }
            CreateRoom { name } => {
                self.handle_collaboration_create_room(name);
            }
            JoinRoom { invite_code } => {
                self.handle_collaboration_join_room(invite_code);
            }
            Disconnect => {
                self.handle_collaboration_disconnect();
            }
            Authenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 认证成功事件 - 用户ID: {}, 目前默认邀请码: {}",
                    user_id,
                    invite_code
                );

                self.window_state
                    .window
                    .ui_mut()
                    .root_mut()
                    .state_mut()
                    .collaboration_dialog
                    .connection_status
                    .clear();

                if let Some(target_invite_code) = self.collab_state.pending_invite_code.take() {
                    tracing::info!("使用首屏填写的邀请码直接加入房间: {}", target_invite_code);
                    self.handle_collaboration_join_room(target_invite_code);
                } else {
                    self.window_state
                        .window
                        .ui_mut()
                        .set_collaboration_view_state(
                            lumino_ui::CollaborationViewState::RoomActions,
                            Some(invite_code),
                            None,
                        );
                }
            }
            RoomCreated {
                room_name,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 房间创建成功 - 房间名: {}, 邀请码: {}",
                    room_name,
                    invite_code
                );
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::InRoom,
                        Some(invite_code),
                        Some(room_name),
                    );
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("协作: 自动关闭协作对话框");
            }
            RoomJoined {
                room_name,
                invite_code,
                user_count,
            } => {
                tracing::info!(
                    "协作: 加入房间成功 - 房间名: {}, 邀请码: {}, 用户数: {}",
                    room_name,
                    invite_code,
                    user_count
                );
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::InRoom,
                        Some(invite_code),
                        Some(room_name),
                    );
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("协作: 自动关闭协作对话框");
            }
            Disconnected => {
                tracing::info!("协作: 连接断开事件");
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::Connect,
                        None,
                        None,
                    );
            }
            MouseUpdate {
                user_id,
                x,
                y,
                color,
                username,
            } => {
                tracing::debug!(
                    "窗口事件 - 远程鼠标更新：user_id={}, x={}, y={}, color={}, username={}",
                    user_id,
                    x,
                    y,
                    color,
                    username
                );
                self.window_state
                    .window
                    .ui_mut()
                    .update_remote_cursor(user_id, x, y, color, username);
                self.window_state.window.window().request_redraw();
            }
            NoteUpdate { user_id, operation } => {
                self.handle_remote_note_update(user_id, operation);
                self.window_state.window.window().request_redraw();
            }
            ProjectUpdate { user_id, update } => {
                self.handle_remote_project_update(user_id, update);
                self.window_state.window.window().request_redraw();
            }
            UserLeft { user_id } => {
                self.window_state
                    .window
                    .ui_mut()
                    .remove_remote_cursor(user_id);
                self.window_state.window.window().request_redraw();
            }
        }
    }

    fn handle_sync_events(&mut self, window_event: lumino_ui::event::window::sync::Event) {
        use lumino_ui::event::window::sync::Event::*;
        match window_event {
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
            LocalNoteMoved {
                tick,
                key,
                length,
                tick_offset,
                key_offset,
                track_index,
            } => {
                self.handle_local_note_moved(
                    tick,
                    key,
                    length,
                    tick_offset,
                    key_offset,
                    track_index,
                );
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

// ── 视频导出辅助函数 ──

/// 从 tempo_changes 计算总时长（秒）
fn compute_duration_secs(tempo_changes: &[(u32, f32)], total_ticks: u32, ppq: u32) -> f64 {
    if tempo_changes.is_empty() || ppq == 0 {
        return total_ticks as f64 / ppq.max(1) as f64 * 0.5; // 120 BPM
    }
    let mut secs = 0.0;
    let mut prev_tick = 0u32;
    let mut prev_bpm = tempo_changes[0].1 as f64;
    for &(tick, bpm) in tempo_changes {
        if tick > prev_tick {
            let delta_ticks = (tick - prev_tick) as f64;
            secs += delta_ticks / ppq as f64 * 60.0 / prev_bpm;
        }
        prev_tick = tick;
        prev_bpm = bpm as f64;
    }
    if total_ticks > prev_tick {
        let delta_ticks = (total_ticks - prev_tick) as f64;
        secs += delta_ticks / ppq as f64 * 60.0 / prev_bpm;
    }
    secs
}

/// 从秒转换到 tick
fn seconds_to_tick(secs: f64, tempo_changes: &[(u32, f32)], ppq: u32) -> u32 {
    if tempo_changes.is_empty() || ppq == 0 {
        return (secs * ppq.max(1) as f64 * 2.0) as u32; // 120 BPM
    }
    let mut remaining = secs;
    let mut prev_tick = 0u32;
    let mut prev_bpm = tempo_changes[0].1 as f64;
    for &(tick, bpm) in tempo_changes {
        if tick > prev_tick {
            let delta_ticks = (tick - prev_tick) as f64;
            let delta_secs = delta_ticks / ppq as f64 * 60.0 / prev_bpm;
            if remaining <= delta_secs {
                return prev_tick + (remaining * ppq as f64 * prev_bpm / 60.0) as u32;
            }
            remaining -= delta_secs;
        }
        prev_tick = tick;
        prev_bpm = bpm as f64;
    }
    prev_tick + (remaining * ppq as f64 * prev_bpm / 60.0) as u32
}

/// 构建视频导出帧的 RenderParams
///
/// 包含编辑区域 UI 元素（网格线、标尺、键盘），
/// Y 向缩放覆盖整个键盘（128 或 256 key），
/// X 向缩放使可见视口内恰好 4 个小节。
fn build_video_render_params(
    width: u32,
    height: u32,
    tick: u32,
    document: &lumino_midi_loader::MidiDocument,
    ppq: u32,
    key_count: u16,
) -> lumino_gfx::RenderParams {
    use lumino_gfx::{
        GridViewParams, KeyInstance, NoteInstance, generate_grid_instances,
        generate_ruler_instances, is_black_key,
    };

    let keyboard_width = 60.0f32;
    let ruler_height = 30.0f32;
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;

    // X 向缩放：视口 tick 范围 = 4 小节
    let viewport_tick_span = (ppq * 16).max(1) as f32;
    let zoom_x = (w - keyboard_width) / viewport_tick_span;

    // Y 向缩放：覆盖整个键盘
    let key_count_f = key_count.max(1) as f32;
    let zoom_y = (h - ruler_height) / key_count_f;

    let scroll_x = tick as f32;
    let scroll_y = 0.0f32;

    // 1. 生成网格线实例
    let grid_params = GridViewParams {
        viewport_width: w,
        viewport_height: h,
        keyboard_width,
        ruler_height,
        scroll_x,
        scroll_y,
        zoom_x,
        zoom_y,
    };
    let grid_instances = generate_grid_instances(&grid_params);

    // 2. 生成标尺实例
    let ruler_instances =
        generate_ruler_instances(w, keyboard_width, ruler_height, scroll_x, zoom_x);

    // 3. 生成键盘实例
    let mut keyboard_instances = Vec::with_capacity(key_count as usize);
    for key in 0..key_count {
        let ky = ruler_height + key as f32 * zoom_y - scroll_y;
        let is_black = is_black_key(key as isize);
        // 保持在视口内
        if ky + zoom_y >= ruler_height && ky <= h {
            keyboard_instances.push(KeyInstance::new(
                [0.0, ky],
                [keyboard_width, zoom_y + 1.0], // +1 防止缝隙
                if is_black {
                    [0.1, 0.1, 0.12, 1.0] // 黑键
                } else {
                    [0.22, 0.22, 0.25, 1.0] // 白键
                },
                is_black,
                key,
            ));
        }
    }

    // 4. 收集可见音符
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);
    let mut note_instances = Vec::new();
    // 蓝色音符
    let color_packed: u32 = (51u32 << 24) | (153u32 << 16) | (255u32 << 8) | 255u32;
    for notes in &document.notes {
        for n in notes {
            if n.end_tick >= tick_start && n.start_tick <= tick_end {
                note_instances.push(NoteInstance {
                    position: [n.start_tick as f32, n.key as f32],
                    size_x: (n.length() as f32).max(1.0),
                    color_packed,
                });
            }
        }
    }

    lumino_gfx::RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (w, h),
        scale_factor: 1.0,
        scroll: (scroll_x, scroll_y),
        zoom: (zoom_x, zoom_y),
        keyboard_width,
        ruler_height,
        note_instances,
        grid_instances,
        ruler_instances,
        keyboard_instances,
        ppq: ppq as f32,
        ..Default::default()
    }
}
