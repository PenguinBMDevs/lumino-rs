//! Runner 窗口事件处理

use std::sync::atomic::{AtomicBool, Ordering};

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_export::audio::config::{AudioChannelMode, AudioInterpolation, ThreadMode};
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
                use lumino_export::audio::config::AudioRenderConfig;
                use std::time::Instant;

                // 根据是否有内存中的 MidiDocument 选择渲染模式
                let mode_str = if document.is_some() {
                    "内存模式（零拷贝）"
                } else {
                    "文件模式"
                };
                tracing::info!("开始音频导出 [{mode_str}]: MIDI={midi_path}, SF2={soundfont_path}");

                let midi_path_buf = PathBuf::from(&midi_path);
                let output_path_buf = PathBuf::from(&output_path);

                // 解析枚举值（来自 UI 的 Debug 格式字符串）
                let channel_mode = match channels.to_ascii_lowercase().as_str() {
                    "mono" => AudioChannelMode::Mono,
                    _ => AudioChannelMode::Stereo,
                };
                let interpolation_val = match interpolation.to_ascii_lowercase().as_str() {
                    "nearest" => AudioInterpolation::Nearest,
                    _ => AudioInterpolation::Linear,
                };
                let channel_threading_val = parse_thread_mode(&channel_threading);
                let key_threading_val = parse_thread_mode(&key_threading);

                let progress_cb: lumino_export::audio::config::ProgressCallback =
                    Arc::new(move |_msg: String, _pct: f64| {
                        tracing::debug!("音频导出进度: {:.1}%", _pct * 100.0);
                    });

                let config = AudioRenderConfig {
                    midi_path: midi_path_buf.clone(),
                    soundfonts: vec![PathBuf::from(&soundfont_path)],
                    output_path: output_path_buf.clone(),
                    sample_rate: sample_rate.max(8000),
                    channels: channel_mode,
                    layer_limit: Some(layer_limit.max(1) as usize),
                    channel_threading: channel_threading_val,
                    key_threading: key_threading_val,
                    interpolation: interpolation_val,
                    apply_limiter,
                    disable_fade_out,
                    linear_envelope,
                    progress_callback: Some(progress_cb),
                };

                let start = Instant::now();
                let render_result = match &document {
                    Some(doc) => lumino_export::audio::render_audio_from_document(&config, doc),
                    None => lumino_export::audio::render_audio(&config),
                };

                match render_result {
                    Ok(_) => {
                        let elapsed = start.elapsed();
                        tracing::info!(
                            "音频导出完成: 耗时 {:.1}s, 输出={}",
                            elapsed.as_secs_f64(),
                            output_path_buf.display()
                        );
                        let main_ui = self.window_state.window.ui_mut();
                        main_ui.set_export_render_completed();
                    }
                    Err(e) => {
                        tracing::error!("音频导出失败: {e}");
                        let main_ui = self.window_state.window.ui_mut();
                        main_ui.set_export_render_failed(format!("音频导出失败: {e}"));
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
                        let _ =
                            progress_tx.send(("导出失败：无 MIDI 数据".to_string(), -1.0, 0, 0.0));
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
                                let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0));
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
                            let _ = progress_tx.send((
                                "导出失败：渲染线程通信错误".to_string(),
                                -1.0,
                                0,
                                0.0,
                            ));
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

                        // 流水线渲染：利用四重缓冲让 GPU 渲染与 CPU 合成/编码重叠
                        // 渲染线程不再每帧 wait_read，inflight 达到上限时才读回
                        // Runner 用 param_queue 跟踪每帧合成参数，try_recv 非阻塞接收
                        let mut param_queue: std::collections::VecDeque<(f32, f32, f32, u32)> =
                            std::collections::VecDeque::new();
                        let mut processed_frames = 0u64;
                        let mut cancelled = false;

                        // 帧处理闭包：composite + write + 统计更新
                        // 返回 true 表示取消或出错
                        // 用内层作用域确保闭包 drop 后 encoder 可被 finish 借用
                        {
                            let mut process_frame = |data: Vec<u8>,
                                                     params: (f32, f32, f32, u32)|
                             -> bool {
                                let mut data = data;
                                let (sx, zx, kw, ppq_val) = params;

                                if data.is_empty() {
                                    tracing::warn!("帧读回为空，跳过");
                                    return false;
                                }

                                if !keyboard_pixels.is_empty() {
                                    video_export::composite_keyboard(
                                        &mut data,
                                        width,
                                        height,
                                        &keyboard_pixels,
                                        kb_w,
                                        kb_h,
                                    );
                                }
                                video_export::composite_ruler_numbers(
                                    &mut data, width, height, sx, zx, kw, ppq_val,
                                );

                                if cancel_flag.load(Ordering::Relaxed) {
                                    tracing::info!("视频导出：帧数据到达后检测到取消，正在收尾...");
                                    let _ = encoder.write_frame(data);
                                    return true;
                                }

                                // 预览帧：在 write_frame（move data）之前 clone 发送
                                if last_preview_time.elapsed()
                                    >= std::time::Duration::from_millis(200)
                                {
                                    // GPU 读回是 BGRA 格式，但 image::Handle::from_rgba 需要 RGBA
                                    let mut preview_data = data.clone();
                                    for pixel in preview_data.chunks_exact_mut(4) {
                                        pixel.swap(0, 2); // B<->R 交换
                                    }
                                    let _ = preview_tx.send((preview_data, width, height));
                                    last_preview_time = std::time::Instant::now();
                                }

                                if let Err(e) = encoder.write_frame(data) {
                                    tracing::error!("写入视频帧失败: {e}");
                                    let _ =
                                        progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0));
                                    return true;
                                }

                                processed_frames += 1;
                                frames_since_stat += 1;
                                let elapsed = last_stat_time.elapsed();
                                if elapsed >= std::time::Duration::from_secs(2) {
                                    let fps = frames_since_stat as f64 / elapsed.as_secs_f64();
                                    smoothed_fps = if smoothed_fps == 0.0 {
                                        fps
                                    } else {
                                        smoothed_fps * 0.7 + fps * 0.3
                                    };
                                    let progress = processed_frames as f64 / total_frames as f64;
                                    let eta_secs =
                                        (total_frames - processed_frames) as f64 / smoothed_fps;
                                    tracing::info!(
                                        "视频导出: 帧 {}/{} ({:.0}%), FPS={:.0}, ETA={:.0}s",
                                        processed_frames,
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
                                        total_frames,
                                        smoothed_fps,
                                    ));
                                    last_stat_time = std::time::Instant::now();
                                    frames_since_stat = 0;
                                }

                                false
                            };

                            // 逐帧渲染：发送命令 + try_recv 处理已就绪的帧
                            for frame in 0..total_frames {
                                if cancel_flag.load(Ordering::Relaxed) {
                                    tracing::info!("视频导出：用户取消，正在收尾...");
                                    cancelled = true;
                                    break;
                                }
                                let time_sec = frame as f64 / fps_f64;
                                let tick =
                                    video_export::seconds_to_tick(time_sec, tempo_changes, ppq);

                                // 计算 scroll_x / zoom_x，用于标尺小节号合成
                                let video_kb_width = 60.0f32;
                                let video_viewport_tick_span = (ppq * 16).max(1) as f32;
                                let video_zoom_x =
                                    (width as f32 - video_kb_width) / video_viewport_tick_span;
                                let video_scroll_x = tick as f32 * video_zoom_x;

                                // 入队帧合成参数（与帧数据 FIFO 对应）
                                param_queue.push_back((
                                    video_scroll_x,
                                    video_zoom_x,
                                    video_kb_width,
                                    ppq,
                                ));

                                let params = video_export::build_video_render_params(
                                    width, height, tick, &document, ppq, key_count,
                                );

                                if cmd_sender
                                    .send(RenderCommand::Control(ControlCommand::RenderVideoFrame(
                                        Box::new(params),
                                    )))
                                    .is_err()
                                {
                                    tracing::error!("发送 RenderVideoFrame 命令失败");
                                    let _ = progress_tx.send((
                                        "导出失败：渲染线程通信错误".to_string(),
                                        -1.0,
                                        0,
                                        0.0,
                                    ));
                                    cancelled = true;
                                    break;
                                }

                                // 处理已就绪的帧数据
                                loop {
                                    let data = if param_queue.len() >= 4 {
                                        // inflight 达到上限，阻塞等最早的一帧
                                        match frame_rx.recv() {
                                            Ok(d) => d,
                                            Err(_) => {
                                                tracing::error!("帧数据通道关闭");
                                                let _ = progress_tx.send((
                                                    "导出失败：帧数据通道关闭".to_string(),
                                                    -1.0,
                                                    0,
                                                    0.0,
                                                ));
                                                cancelled = true;
                                                break;
                                            }
                                        }
                                    } else {
                                        // 非阻塞接收
                                        match frame_rx.try_recv() {
                                            Ok(d) => d,
                                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                                tracing::error!("帧数据通道关闭");
                                                let _ = progress_tx.send((
                                                    "导出失败：帧数据通道关闭".to_string(),
                                                    -1.0,
                                                    0,
                                                    0.0,
                                                ));
                                                cancelled = true;
                                                break;
                                            }
                                        }
                                    };

                                    let p =
                                        param_queue.pop_front().unwrap_or((0.0, 1.0, 60.0, ppq));
                                    if process_frame(data, p) {
                                        cancelled = true;
                                        break;
                                    }
                                }

                                if cancelled {
                                    break;
                                }
                            }

                            // drain 剩余 inflight 帧
                            while !param_queue.is_empty() && !cancelled {
                                match frame_rx.recv() {
                                    Ok(data) => {
                                        let p = param_queue
                                            .pop_front()
                                            .unwrap_or((0.0, 1.0, 60.0, ppq));
                                        if process_frame(data, p) {
                                            cancelled = true;
                                        }
                                    }
                                    Err(_) => {
                                        tracing::error!("drain 阶段帧数据通道关闭");
                                        cancelled = true;
                                    }
                                }
                            }
                        } // process_frame 闭包 drop，释放 encoder 借用

                        // 完成编码：无论是否取消都必须调用 finish()，
                        // 否则 FFmpeg 收不到 EOF，视频文件头未写入导致损坏。
                        // 用户取消时已写入的帧仍可生成可播放的部分视频。
                        let elapsed = start.elapsed();
                        if !cancelled {
                            tracing::info!("视频导出完成: 耗时 {:.1}s", elapsed.as_secs_f64());
                            let _ = progress_tx.send((
                                "导出完成".to_string(),
                                1.0,
                                total_frames,
                                smoothed_fps,
                            ));
                        } else {
                            tracing::info!(
                                "视频导出取消: 已处理 {} 帧, 耗时 {:.1}s, 正在收尾编码器",
                                processed_frames,
                                elapsed.as_secs_f64()
                            );
                            let _ = progress_tx.send((
                                "导出已取消".to_string(),
                                1.0,
                                total_frames,
                                smoothed_fps,
                            ));
                        }
                        if let Err(e) = encoder.finish() {
                            tracing::error!("FFmpeg 收尾失败: {e}");
                            let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0));
                        }
                    });
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
            Connect {
                host,
                port,
                username,
                invite_code,
            } => {
                tracing::info!("请求连接协作服务器: {host}:{port}");
                self.handle_collaboration_connect(host, port, username, None, invite_code);
            }
            CreateRoom { name } => {
                tracing::info!("请求创建协作房间: {name}");
                self.handle_collaboration_create_room(name);
            }
            JoinRoom { invite_code } => {
                tracing::info!("请求加入协作房间: {invite_code}");
                self.handle_collaboration_join_room(invite_code);
            }
            Disconnect => {
                tracing::info!("请求断开协作连接");
                self.handle_collaboration_disconnect();
            }
            Authenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!("协作认证成功: user={user_id}, invite={invite_code}");
            }
            RoomCreated {
                room_name,
                invite_code,
            } => {
                tracing::info!("协作房间创建成功: {room_name}, invite={invite_code}");
            }
            RoomJoined {
                room_name,
                invite_code,
                user_count,
            } => {
                tracing::info!(
                    "已加入协作房间: {room_name}, invite={invite_code}, 用户数={user_count}"
                );
            }
            Disconnected => {
                tracing::info!("协作连接已断开");
            }
            UserLeft { user_id } => {
                tracing::info!("协作用户离开: {user_id}");
            }
            MouseUpdate { user_id, x, y, .. } => {
                tracing::debug!("协作鼠标更新: user={user_id}, ({x:.0},{y:.0})");
            }
            NoteUpdate { user_id, operation } => {
                self.handle_remote_note_update(user_id, operation);
            }
            ProjectUpdate { user_id, update } => {
                self.handle_remote_project_update(user_id, update);
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

/// 解析 UI 传来的线程模式字符串为 ThreadMode 枚举
fn parse_thread_mode(s: &str) -> ThreadMode {
    let lower = s.to_ascii_lowercase();
    if lower == "none" || lower.is_empty() {
        ThreadMode::None
    } else if lower == "auto" {
        ThreadMode::Auto
    } else if let Ok(n) = lower.parse::<u32>() {
        ThreadMode::Manual(n)
    } else {
        ThreadMode::None
    }
}
