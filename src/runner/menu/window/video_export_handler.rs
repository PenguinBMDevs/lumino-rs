//! 视频导出窗口事件处理：在后台线程执行逐帧渲染 + FFmpeg 编码，进度通过通道回传主线程。

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{channel, TryRecvError};

use lumino_export::video::{
    FfmpegEncoder, VideoExportConfig,
    config::{Container, EncoderBackend, QualityPreset, VideoCodec},
};
use lumino_gfx::render_thread::{ControlCommand, FrameSender, RenderCommand};

impl RunnerInner {
    pub(crate) fn handle_start_video_export(
        &mut self,
        config: lumino_event::window::video::VideoExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) {
        let lumino_event::window::video::VideoExportConfig {
            output_path,
            width,
            height,
            fps,
            ppq,
            key_count,
            container,
            codec,
            backend,
            quality,
            render_mode,
        } = config;

        // 事件层枚举 → 导出层枚举（总映射，无字符串解析、无静默降级）
        let container = match container {
            lumino_event::window::video::Container::Mp4 => Container::Mp4,
            lumino_event::window::video::Container::Mov => Container::Mov,
            lumino_event::window::video::Container::Mkv => Container::Mkv,
            lumino_event::window::video::Container::Avi => Container::Avi,
        };
        let codec = match codec {
            lumino_event::window::video::VideoCodec::H264 => VideoCodec::H264,
            lumino_event::window::video::VideoCodec::H265 => VideoCodec::H265,
            lumino_event::window::video::VideoCodec::ProRes => VideoCodec::ProRes,
            lumino_event::window::video::VideoCodec::Vp9 => VideoCodec::Vp9,
            lumino_event::window::video::VideoCodec::Av1 => VideoCodec::Av1,
        };
        let backend = match backend {
            lumino_event::window::video::EncoderBackend::Software => EncoderBackend::Software,
            lumino_event::window::video::EncoderBackend::VideoToolbox => {
                EncoderBackend::VideoToolbox
            }
            lumino_event::window::video::EncoderBackend::Nvenc => EncoderBackend::Nvenc,
            lumino_event::window::video::EncoderBackend::Amf => EncoderBackend::Amf,
            lumino_event::window::video::EncoderBackend::Qsv => EncoderBackend::Qsv,
            lumino_event::window::video::EncoderBackend::Vaapi => EncoderBackend::Vaapi,
        };
        let quality = match quality {
            lumino_event::window::video::QualityPreset::High => QualityPreset::High,
            lumino_event::window::video::QualityPreset::Medium => QualityPreset::Medium,
            lumino_event::window::video::QualityPreset::Low => QualityPreset::Low,
        };

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
            container,
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
                let _ = progress_tx.send(("导出失败：无 MIDI 数据".to_string(), -1.0, 0, 0.0, 0.0));
                return;
            }
        };

        let ppq = ppq.max(1) as u32;
        let fps_f64 = fps as f64;
        let render_mode_for_thread = render_mode;

        // 预先提取 UI 配置中 HiRes 相关字段，避免将非 Send 的 self 捕获进后台线程
        let hires_video_config =
            if render_mode_for_thread == lumino_event::window::video::RenderMode::HiResTexture {
                let ui_config = &self.window_state.storage.config.get().ui;
                Some(super::video_export::build_hires_config_for_video(ui_config))
            } else {
                None
            };
        let hires_key_count = if self.window_state.storage.config.get().ui.enable_256key {
            256
        } else {
            128
        };

        // 创建取消标志
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.window_state.video_export_cancel = cancel_flag.clone();

        // 后台线程：逐帧渲染 + FFmpeg 编码
        let _ = std::thread::Builder::new()
            .name("video-render".into())
            .spawn(move || {
                let start = std::time::Instant::now();

                // 创建 FFmpeg 编码器
                let mut encoder = match FfmpegEncoder::new(&config) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::error!("FFmpeg 创建失败: {e}");
                        let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
                        return;
                    }
                };

                // 创建帧数据通道
                let (frame_tx, frame_rx) = channel::<Vec<u8>>();

                // 若使用 HiRes 贴图模式，先在 Runner 线程生成并上传到 GPU
                if let Some(hires_config) = hires_video_config {
                    let ppq_u16 = ppq.max(1).min(u16::MAX as u32) as u16;
                    let tiles_map = super::video_export::generate_hires_video_tiles(
                        &document,
                        &hires_config,
                        ppq_u16,
                        hires_key_count,
                    );
                    let tiles: Vec<lumino_gfx::GroupTile> = tiles_map.into_values().collect();
                    let track_count = document.notes.len() as u16;
                    if cmd_sender
                        .send(RenderCommand::Control(
                            ControlCommand::UploadHiResVideoTiles {
                                tiles,
                                config: hires_config,
                                track_count,
                                key_count: hires_key_count,
                                total_ticks: document.total_ticks,
                                ppq: ppq_u16,
                            },
                        ))
                        .is_err()
                    {
                        tracing::error!("发送 UploadHiResVideoTiles 命令失败");
                        let _ = progress_tx.send((
                            "导出失败：渲染线程通信错误".to_string(),
                            -1.0,
                            0,
                            0.0,
                            0.0,
                        ));
                        return;
                    }
                    tracing::info!("视频导出: HiRes 贴图已上传");
                }

                // 发送 StartVideoExport 命令
                if cmd_sender
                    .send(RenderCommand::Control(ControlCommand::StartVideoExport {
                        width,
                        height,
                        frame_tx: FrameSender(frame_tx),
                        render_mode: render_mode_for_thread.as_str().to_string(),
                    }))
                    .is_err()
                {
                    tracing::error!("发送 StartVideoExport 命令失败");
                    let _ = progress_tx.send((
                        "导出失败：渲染线程通信错误".to_string(),
                        -1.0,
                        0,
                        0.0,
                        0.0,
                    ));
                    return;
                }

                // 计算总帧数
                let tempo_changes = &document.tempo_changes;
                let total_ticks = document.total_ticks;
                let duration_secs =
                    super::video_export::compute_duration_secs(tempo_changes, total_ticks, ppq);
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
                let mut preview_sent = false;

                // ★ 生成键盘贴图（使用 CPU 贴图方式，在帧数据上合成）
                let (keyboard_pixels, kb_w, kb_h) =
                    super::video_export::generate_keyboard_texture(width, height, key_count);

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
                    let mut process_frame = |data: Vec<u8>, params: (f32, f32, f32, u32)| -> bool {
                        let mut data = data;
                        let (sx, zx, kw, ppq_val) = params;

                        if data.is_empty() {
                            tracing::warn!("帧读回为空，跳过");
                            return false;
                        }

                        if !keyboard_pixels.is_empty() {
                            super::video_export::composite_keyboard(
                                &mut data,
                                width,
                                height,
                                &keyboard_pixels,
                                kb_w,
                                kb_h,
                            );
                        }
                        super::video_export::composite_ruler_numbers(
                            &mut data, width, height, sx, zx, kw, ppq_val,
                        );

                        if cancel_flag.load(Ordering::Relaxed) {
                            tracing::info!("视频导出：帧数据到达后检测到取消，正在收尾...");
                            let _ = encoder.write_frame(data);
                            return true;
                        }

                        // 预览帧：在 write_frame（move data）之前 clone 发送。
                        // 第一帧立即发送，让预览界面尽快有内容；后续按 200ms 节流。
                        if !preview_sent
                            || last_preview_time.elapsed() >= std::time::Duration::from_millis(200)
                        {
                            // GPU 读回是 BGRA 格式，但 image::Handle::from_rgba 需要 RGBA
                            let mut preview_data = data.clone();
                            for pixel in preview_data.chunks_exact_mut(4) {
                                pixel.swap(0, 2); // B<->R 交换
                            }

                            // 缩小预览到 ≤480px 宽，确保像素数据 <2MB，
                            // 让 iced_wgpu 走同步上传路径而非异步后台上传
                            const PREVIEW_MAX_W: u32 = 480;
                            let (small_data, small_w, small_h) = if width > PREVIEW_MAX_W {
                                let scale = PREVIEW_MAX_W as f64 / width as f64;
                                let tw = PREVIEW_MAX_W;
                                let th = (height as f64 * scale).round() as u32;
                                super::downscale_rgba(&preview_data, width, height, tw, th)
                            } else {
                                (preview_data, width, height)
                            };

                            tracing::info!(
                                "视频导出: 发送预览帧 {}x{} ({} bytes), 首帧={}",
                                small_w,
                                small_h,
                                small_data.len(),
                                !preview_sent
                            );
                            if preview_tx.send((small_data, small_w, small_h)).is_err() {
                                tracing::warn!("视频导出: 预览帧发送失败，接收端已关闭");
                            }
                            last_preview_time = std::time::Instant::now();
                            preview_sent = true;
                        }

                        if let Err(e) = encoder.write_frame(data) {
                            tracing::error!("写入视频帧失败: {e}");
                            let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
                            return true;
                        }

                        processed_frames += 1;
                        frames_since_stat += 1;
                        let elapsed = last_stat_time.elapsed();
                        if elapsed >= std::time::Duration::from_millis(100) {
                            let fps = frames_since_stat as f64 / elapsed.as_secs_f64();
                            smoothed_fps = if smoothed_fps == 0.0 {
                                fps
                            } else {
                                smoothed_fps * 0.7 + fps * 0.3
                            };
                            let progress = processed_frames as f64 / total_frames as f64;
                            let eta_secs = (total_frames - processed_frames) as f64 / smoothed_fps;
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
                                0.0, // 进度更新中不传递 elapsed
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
                        let tick = super::video_export::seconds_to_tick(time_sec, tempo_changes, ppq);

                        // 计算 scroll_x / zoom_x，用于标尺小节号合成
                        let video_kb_width = 60.0f32;
                        let video_viewport_tick_span = (ppq * 16).max(1) as f32;
                        let video_zoom_x =
                            (width as f32 - video_kb_width) / video_viewport_tick_span;
                        let video_scroll_x = tick as f32 * video_zoom_x;

                        // 入队帧合成参数（与帧数据 FIFO 对应）
                        param_queue.push_back((video_scroll_x, video_zoom_x, video_kb_width, ppq));

                        let params = super::video_export::build_video_render_params(
                            width, height, tick, &document, ppq, key_count,
                        );

                        if cmd_sender
                            .send(RenderCommand::Control(ControlCommand::RenderVideoFrame {
                                params: Box::new(params),
                                render_mode: render_mode_for_thread.as_str().to_string(),
                            }))
                            .is_err()
                        {
                            tracing::error!("发送 RenderVideoFrame 命令失败");
                            let _ = progress_tx.send((
                                "导出失败：渲染线程通信错误".to_string(),
                                -1.0,
                                0,
                                0.0,
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
                                    Err(TryRecvError::Empty) => break,
                                    Err(TryRecvError::Disconnected) => {
                                        tracing::error!("帧数据通道关闭");
                                        let _ = progress_tx.send((
                                            "导出失败：帧数据通道关闭".to_string(),
                                            -1.0,
                                            0,
                                            0.0,
                                            0.0,
                                        ));
                                        cancelled = true;
                                        break;
                                    }
                                }
                            };

                            let p = param_queue.pop_front().unwrap_or((0.0, 1.0, 60.0, ppq));
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
                                let p = param_queue.pop_front().unwrap_or((0.0, 1.0, 60.0, ppq));
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
                        elapsed.as_secs_f64(),
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
                        elapsed.as_secs_f64(),
                    ));
                }
                if let Err(e) = encoder.finish() {
                    tracing::error!("FFmpeg 收尾失败: {e}");
                    let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
                }
            });
    }
}
