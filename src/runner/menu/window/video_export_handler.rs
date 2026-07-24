//! 视频导出窗口事件处理：在后台线程执行逐帧渲染 + FFmpeg 编码，进度通过通道回传主线程。

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use lumino_export::video::{
    FfmpegEncoder, VideoExportConfig,
    config::{Container, EncoderBackend, QualityPreset, VideoCodec},
};
use lumino_gfx::render_thread::{ControlCommand, FrameSender, RenderCommand};

use super::video_export::streaming::StreamingNoteSource;

impl RunnerInner {
    pub(crate) fn handle_start_video_export(
        &mut self,
        config: lumino_event::window::video::VideoExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) {
        let lumino_event::window::video::VideoExportConfig {
            output_path,
            midi_path,
            width,
            height,
            fps,
            ppq,
            key_count,
            container,
            codec,
            backend,
            quality,
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

        // 获取渲染线程命令发送端与纹理格式
        let main_ui = self.window_state.window.ui();
        let cmd_sender = main_ui.render_command_sender();
        let input_pix_fmt =
            match main_ui.texture_format() {
                lumino_gfx::TextureFormat::Bgra8Unorm
                | lumino_gfx::TextureFormat::Bgra8UnormSrgb => "bgra",
                lumino_gfx::TextureFormat::Rgba8Unorm
                | lumino_gfx::TextureFormat::Rgba8UnormSrgb => "rgba",
                _ => "bgra",
            };

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

        let ppq = ppq.max(1) as u32;
        let fps_f64 = fps as f64;

        // 创建取消标志
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.window_state.video_export_cancel = cancel_flag.clone();

        // 后台线程：逐帧渲染 + FFmpeg 编码
        let _ = std::thread::Builder::new()
            .name("video-render".into())
            .spawn(move || {
                if let Some(document) = document {
                    run_video_export_task(
                        config,
                        cmd_sender,
                        progress_tx,
                        preview_tx,
                        document,
                        ppq,
                        fps_f64,
                        key_count,
                        width,
                        height,
                        cancel_flag,
                        input_pix_fmt,
                    );
                } else if !midi_path.is_empty() {
                    run_streaming_video_export_task(
                        config,
                        cmd_sender,
                        progress_tx,
                        preview_tx,
                        midi_path,
                        fps_f64,
                        key_count,
                        width,
                        height,
                        cancel_flag,
                        input_pix_fmt,
                    );
                } else {
                    tracing::error!("视频导出失败：无 MidiDocument 且未指定 MIDI 路径");
                    let _ =
                        progress_tx.send(("导出失败：无 MIDI 数据".to_string(), -1.0, 0, 0.0, 0.0));
                }
            });
    }
}

/// 后台线程主流程：创建编码器、发送初始渲染命令、逐帧渲染 + 编码、收尾。
///
/// 该函数整体等价于原 `handle_start_video_export` 中 `move` 闭包体内的逻辑，
/// 仅将各阶段进一步拆分成下方私有步骤函数，行为保持一致。
#[allow(clippy::too_many_arguments)]
fn run_video_export_task(
    config: lumino_export::video::VideoExportConfig,
    cmd_sender: std::sync::mpsc::Sender<lumino_gfx::render_thread::RenderCommand>,
    progress_tx: tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
    preview_tx: tokio::sync::mpsc::UnboundedSender<(Vec<u8>, u32, u32)>,
    document: Arc<lumino_midi_loader::MidiDocument>,
    ppq: u32,
    fps_f64: f64,
    key_count: u16,
    width: u32,
    height: u32,
    cancel_flag: Arc<AtomicBool>,
    input_pix_fmt: &'static str,
) {
    let start = std::time::Instant::now();

    // 按键颜色增量扫描状态（与编辑器 PlaybackScanState 等价）
    let mut key_color_state = super::video_export::keyboard::PlaybackKeyColorState::default();
    let mut key_colors = [0u8; super::video_export::keyboard::KEY_COLOR_BYTES];

    // 创建帧数据通道与回收通道
    let (frame_tx, frame_rx) = channel::<Vec<u8>>();
    let (recycle_tx, recycle_rx) = channel::<Vec<u8>>();

    // 创建 FFmpeg 编码器（直连写入模式，缓冲区由调用方在 write_frame 后归还对象池）
    let mut encoder = match FfmpegEncoder::new(&config, input_pix_fmt) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("FFmpeg 创建失败: {e}");
            let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
            return;
        }
    };

    // 发送初始渲染命令（StartVideoExport），携带帧缓冲回收通道
    if send_initial_render_commands(
        &cmd_sender,
        width,
        height,
        frame_tx,
        recycle_rx,
        &progress_tx,
    ) {
        return;
    }

    // 计算总帧数
    let tempo_changes = &document.tempo_changes;
    let total_ticks = document.total_ticks;
    let duration_secs = super::video_export::compute_duration_secs(tempo_changes, total_ticks, ppq);
    let total_frames = config.total_frames(duration_secs);

    tracing::info!(
        "视频导出: 总时长 {:.1}s, 总帧数 {}, PPQ {}",
        duration_secs,
        total_frames,
        ppq
    );

    let mut last_stat_time = Instant::now();
    let mut frames_since_stat = 0u64;
    let mut smoothed_fps = 0.0f64;
    let mut last_preview_time = Instant::now();
    let mut preview_sent = false;

    // ★ 生成键盘贴图（使用 CPU 贴图方式，在帧数据上合成）
    let (keyboard_pixels, kb_w, kb_h) =
        super::video_export::generate_keyboard_texture(width, height, key_count);

    // 流水线渲染：Runner 预填充 4 帧命令，让 staging ring 从开始就满载，
    // 之后每处理完一帧立即补发下一帧，保持 GPU/CPU 流水线持续运转。
    // 每帧参数携带该帧的按键高亮颜色（RGBAx256 键），用于后台线程合成键盘。
    let mut param_queue: std::collections::VecDeque<(f32, f32, f32, u32, [u8; 1024])> =
        std::collections::VecDeque::new();
    let mut processed_frames = 0u64;
    let mut cancelled = false;
    let mut next_frame_to_send = 0u64;
    const PIPELINE_DEPTH: usize = 4;

    // 各阶段耗时累加器（微秒），用于每 100ms 输出阶段打点日志
    let mut acc_recv_us = 0u64;
    let mut acc_composite_us = 0u64;
    let mut acc_preview_us = 0u64;
    let mut acc_encode_us = 0u64;
    let mut stat_frame_count = 0u64;

    // 闭包不捕获 param_queue，而是作为参数传入，避免与主循环中的 pop_front 产生可变借用冲突。
    let mut enqueue_frame =
        |queue: &mut std::collections::VecDeque<(f32, f32, f32, u32, [u8; 1024])>,
         frame_idx: u64|
         -> bool {
            let time_sec = frame_idx as f64 / fps_f64;
            let tick = super::video_export::seconds_to_tick(time_sec, tempo_changes, ppq);

            // 根据当前播放 tick 增量计算按键高亮颜色
            super::video_export::keyboard::update_playback_key_colors(
                &document,
                tick,
                &mut key_color_state,
                &mut key_colors,
            );

            // 计算 scroll_x / zoom_x，用于标尺小节号合成
            let video_kb_width = 60.0f32;
            let video_viewport_tick_span = (ppq * 16).max(1) as f32;
            let video_zoom_x = (width as f32 - video_kb_width) / video_viewport_tick_span;
            let video_scroll_x = tick as f32 * video_zoom_x;

            // 入队帧合成参数（与帧数据 FIFO 对应）
            queue.push_back((
                video_scroll_x,
                video_zoom_x,
                video_kb_width,
                ppq,
                key_colors,
            ));

            let params = super::video_export::build_video_render_params(
                width, height, tick, &document, ppq, key_count,
            );

            if cmd_sender
                .send(RenderCommand::Control(ControlCommand::RenderVideoFrame {
                    params: Box::new(params),
                }))
                .is_err()
            {
                tracing::error!("发送 RenderVideoFrame 命令失败");
                let _ =
                    progress_tx.send(("导出失败：渲染线程通信错误".to_string(), -1.0, 0, 0.0, 0.0));
                return true;
            }
            false
        };

    // 预填充 inflight，让 GPU 从第一帧就进入流水线满载状态
    for _ in 0..PIPELINE_DEPTH.min(total_frames as usize) {
        if cancel_flag.load(Ordering::Relaxed) {
            tracing::info!("视频导出：用户取消，正在收尾...");
            cancelled = true;
            break;
        }
        if enqueue_frame(&mut param_queue, next_frame_to_send) {
            cancelled = true;
            break;
        }
        next_frame_to_send += 1;
    }

    // 主循环：每收到一帧就合成/编码，并立即补发下一帧命令
    while processed_frames < total_frames && !cancelled {
        if cancel_flag.load(Ordering::Relaxed) {
            tracing::info!("视频导出：用户取消，正在收尾...");
            cancelled = true;
            break;
        }

        let recv_start = Instant::now();
        let data = match frame_rx.recv() {
            Ok(d) => d,
            Err(_) => {
                tracing::error!("帧数据通道关闭");
                let _ =
                    progress_tx.send(("导出失败：帧数据通道关闭".to_string(), -1.0, 0, 0.0, 0.0));
                cancelled = true;
                break;
            }
        };
        let recv_us = recv_start.elapsed().as_micros() as u64;

        let p = param_queue
            .pop_front()
            .unwrap_or((0.0, 1.0, 60.0, ppq, [0u8; 1024]));
        let (should_stop, stats) = composite_and_encode_frame(
            data,
            p,
            &mut encoder,
            &progress_tx,
            &preview_tx,
            &cancel_flag,
            &mut last_preview_time,
            &mut preview_sent,
            width,
            height,
            &keyboard_pixels,
            kb_w,
            kb_h,
            &recycle_tx,
        );

        acc_recv_us += recv_us;
        acc_composite_us += stats.composite_us;
        acc_preview_us += stats.preview_us;
        acc_encode_us += stats.encode_us;
        stat_frame_count += 1;

        if should_stop {
            cancelled = true;
            break;
        }

        processed_frames += 1;
        frames_since_stat += 1;

        // 维持流水线深度：每处理完一帧立即补发下一帧命令
        if next_frame_to_send < total_frames {
            if enqueue_frame(&mut param_queue, next_frame_to_send) {
                cancelled = true;
                break;
            }
            next_frame_to_send += 1;
        }

        // 阶段耗时打点：每 100ms 聚合输出一次
        if last_stat_time.elapsed() >= Duration::from_millis(100) && stat_frame_count > 0 {
            let elapsed = last_stat_time.elapsed().as_secs_f64();
            let fps = frames_since_stat as f64 / elapsed;
            smoothed_fps = if smoothed_fps == 0.0 {
                fps
            } else {
                smoothed_fps * 0.7 + fps * 0.3
            };
            let progress = processed_frames as f64 / total_frames as f64;
            let eta_secs = (total_frames - processed_frames) as f64 / smoothed_fps;
            let avg_recv = acc_recv_us / stat_frame_count;
            let avg_composite = acc_composite_us / stat_frame_count;
            let avg_preview = acc_preview_us / stat_frame_count;
            let avg_encode = acc_encode_us / stat_frame_count;
            tracing::info!(
                "视频导出: 帧 {}/{} ({:.0}%), FPS={:.0}, ETA={:.0}s, 阶段耗时(us) recv={} composite={} preview={} encode={}",
                processed_frames,
                total_frames,
                progress * 100.0,
                smoothed_fps,
                eta_secs,
                avg_recv,
                avg_composite,
                avg_preview,
                avg_encode,
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
            last_stat_time = Instant::now();
            frames_since_stat = 0;
            acc_recv_us = 0;
            acc_composite_us = 0;
            acc_preview_us = 0;
            acc_encode_us = 0;
            stat_frame_count = 0;
        }
    }

    // drain 剩余 inflight 帧
    while !param_queue.is_empty() && !cancelled {
        let recv_start = Instant::now();
        let data = match frame_rx.recv() {
            Ok(d) => d,
            Err(_) => {
                tracing::error!("drain 阶段帧数据通道关闭");
                cancelled = true;
                break;
            }
        };
        let recv_us = recv_start.elapsed().as_micros() as u64;

        let p = param_queue
            .pop_front()
            .unwrap_or((0.0, 1.0, 60.0, ppq, [0u8; 1024]));
        let (should_stop, stats) = composite_and_encode_frame(
            data,
            p,
            &mut encoder,
            &progress_tx,
            &preview_tx,
            &cancel_flag,
            &mut last_preview_time,
            &mut preview_sent,
            width,
            height,
            &keyboard_pixels,
            kb_w,
            kb_h,
            &recycle_tx,
        );

        acc_recv_us += recv_us;
        acc_composite_us += stats.composite_us;
        acc_preview_us += stats.preview_us;
        acc_encode_us += stats.encode_us;
        stat_frame_count += 1;

        if should_stop {
            cancelled = true;
            break;
        }
        processed_frames += 1;
    }

    // 输出最后一组阶段打点（如果有未聚合的数据）
    if let Some(divisor) = std::num::NonZeroU64::new(stat_frame_count) {
        let d = divisor.get();
        tracing::info!(
            "视频导出: 收尾阶段耗时(us) recv={} composite={} preview={} encode={}",
            acc_recv_us / d,
            acc_composite_us / d,
            acc_preview_us / d,
            acc_encode_us / d,
        );
    }

    // 完成编码：无论是否取消都必须调用 finish()，
    // 否则 FFmpeg 收不到 EOF，视频文件头未写入导致损坏。
    // 用户取消时已写入的帧仍可生成可播放的部分视频。
    let elapsed = start.elapsed().as_secs_f64();
    finalize_video_export(
        encoder,
        cancelled,
        processed_frames,
        elapsed,
        total_frames,
        smoothed_fps,
        &progress_tx,
    );
}

/// 流式 MIDI 视频导出后台任务。
///
/// 1. 解析 MIDI 文件并写入硬盘缓存，同时通过 `progress_tx` 回传解析进度。
/// 2. 打开流式音符数据源，按帧 seek+read 读取可见音符。
/// 3. 其余渲染/编码/合成流程与内存模式保持一致。
#[allow(clippy::too_many_arguments)]
fn run_streaming_video_export_task(
    config: lumino_export::video::VideoExportConfig,
    cmd_sender: std::sync::mpsc::Sender<lumino_gfx::render_thread::RenderCommand>,
    progress_tx: tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
    preview_tx: tokio::sync::mpsc::UnboundedSender<(Vec<u8>, u32, u32)>,
    midi_path: String,
    fps_f64: f64,
    key_count: u16,
    width: u32,
    height: u32,
    cancel_flag: Arc<AtomicBool>,
    input_pix_fmt: &'static str,
) {
    let start = std::time::Instant::now();

    // 阶段 1：解析 MIDI → 硬盘缓存
    let progress_tx_for_parse = progress_tx.clone();
    let parse_progress: std::sync::Arc<dyn Fn(String, f64) + Send + Sync> =
        std::sync::Arc::new(move |message: String, value: f64| {
            // 解析阶段进度映射到 0.0 ~ 0.3，与渲染阶段 0.3 ~ 1.0 衔接
            let scaled = value * 0.3;
            let _ = progress_tx_for_parse.send((message, scaled, 0, 0.0, 0.0));
        });

    let parse_result = super::video_export::streaming::parse_midi_to_cache(
        std::path::Path::new(&midi_path),
        fps_f64,
        16.0, // 视口小节数，与内存模式一致（ppq * 16）
        Some(parse_progress),
    );

    let streaming_result = match parse_result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("视频导出 MIDI 解析失败: {e}");
            let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
            return;
        }
    };

    let mut source = match StreamingNoteSource::open(streaming_result) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("视频导出打开流式数据源失败: {e}");
            let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
            return;
        }
    };

    let ppq = source.ppqn();
    let total_frames = source.total_frames();
    let total_ticks = source.total_ticks();
    let duration_secs = source.compute_duration_secs();

    tracing::info!(
        "视频导出流式模式: 总时长 {:.1}s, 总帧数 {}, PPQN {}, total_ticks {}",
        duration_secs,
        total_frames,
        ppq,
        total_ticks
    );

    // 创建帧数据通道与回收通道
    let (frame_tx, frame_rx) = channel::<Vec<u8>>();
    let (recycle_tx, recycle_rx) = channel::<Vec<u8>>();

    // 创建 FFmpeg 编码器
    let mut encoder = match FfmpegEncoder::new(&config, input_pix_fmt) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("FFmpeg 创建失败: {e}");
            let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
            return;
        }
    };

    // 发送初始渲染命令
    if send_initial_render_commands(
        &cmd_sender,
        width,
        height,
        frame_tx,
        recycle_rx,
        &progress_tx,
    ) {
        return;
    }

    // 生成键盘贴图
    let (keyboard_pixels, kb_w, kb_h) =
        super::video_export::generate_keyboard_texture(width, height, key_count);

    let mut last_stat_time = Instant::now();
    let mut frames_since_stat = 0u64;
    let mut smoothed_fps = 0.0f64;
    let mut last_preview_time = Instant::now();
    let mut preview_sent = false;

    let mut param_queue: std::collections::VecDeque<(f32, f32, f32, u32, [u8; 1024])> =
        std::collections::VecDeque::new();
    let mut processed_frames = 0u64;
    let mut cancelled = false;
    let mut next_frame_to_send = 0u64;
    const PIPELINE_DEPTH: usize = 4;

    let mut acc_recv_us = 0u64;
    let mut acc_composite_us = 0u64;
    let mut acc_preview_us = 0u64;
    let mut acc_encode_us = 0u64;
    let mut stat_frame_count = 0u64;

    // 入队闭包：读取流式音符、计算键色、发送渲染命令
    {
        let mut enqueue_frame =
            |queue: &mut std::collections::VecDeque<(f32, f32, f32, u32, [u8; 1024])>,
             frame_idx: u64|
             -> bool {
                let (notes, params) = match source
                    .read_notes_and_params_for_frame(frame_idx, width, height, fps_f64)
                {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!("读取流式音符失败: {e}");
                        let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
                        return true;
                    }
                };

                let tick = super::video_export::seconds_to_tick(
                    frame_idx as f64 / fps_f64,
                    source.tempo_changes(),
                    source.ppqn(),
                );

                // 计算按键高亮颜色
                let mut key_colors = [0u8; super::video_export::keyboard::KEY_COLOR_BYTES];
                let note_tuples: Vec<(u32, u32, u16, u16)> = notes
                    .iter()
                    .map(|n| (n.start_tick, n.end_tick, n.key, n.track))
                    .collect();
                super::video_export::keyboard::update_playback_key_colors_from_notes(
                    &note_tuples,
                    tick,
                    &mut key_colors,
                );

                // 计算 scroll_x / zoom_x，用于标尺小节号合成
                let video_kb_width = 60.0f32;
                let video_viewport_tick_span = (ppq * 16).max(1) as f32;
                let video_zoom_x = (width as f32 - video_kb_width) / video_viewport_tick_span;
                let video_scroll_x = tick as f32 * video_zoom_x;

                queue.push_back((
                    video_scroll_x,
                    video_zoom_x,
                    video_kb_width,
                    ppq,
                    key_colors,
                ));

                if cmd_sender
                    .send(RenderCommand::Control(ControlCommand::RenderVideoFrame {
                        params: Box::new(params),
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
                    return true;
                }
                false
            };

        // 预填充流水线
        for _ in 0..PIPELINE_DEPTH.min(total_frames as usize) {
            if cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("视频导出：用户取消，正在收尾...");
                cancelled = true;
                break;
            }
            if enqueue_frame(&mut param_queue, next_frame_to_send) {
                cancelled = true;
                break;
            }
            next_frame_to_send += 1;
        }

        // 主循环
        while processed_frames < total_frames && !cancelled {
            if cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("视频导出：用户取消，正在收尾...");
                cancelled = true;
                break;
            }

            let recv_start = Instant::now();
            let data = match frame_rx.recv() {
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
            };
            let recv_us = recv_start.elapsed().as_micros() as u64;

            let p = param_queue
                .pop_front()
                .unwrap_or((0.0, 1.0, 60.0, ppq, [0u8; 1024]));
            let (should_stop, stats) = composite_and_encode_frame(
                data,
                p,
                &mut encoder,
                &progress_tx,
                &preview_tx,
                &cancel_flag,
                &mut last_preview_time,
                &mut preview_sent,
                width,
                height,
                &keyboard_pixels,
                kb_w,
                kb_h,
                &recycle_tx,
            );

            acc_recv_us += recv_us;
            acc_composite_us += stats.composite_us;
            acc_preview_us += stats.preview_us;
            acc_encode_us += stats.encode_us;
            stat_frame_count += 1;

            if should_stop {
                cancelled = true;
                break;
            }

            processed_frames += 1;
            frames_since_stat += 1;

            // 维持流水线深度
            if next_frame_to_send < total_frames {
                if enqueue_frame(&mut param_queue, next_frame_to_send) {
                    cancelled = true;
                    break;
                }
                next_frame_to_send += 1;
            }

            // 阶段耗时打点：每 100ms 聚合输出一次
            if last_stat_time.elapsed() >= Duration::from_millis(100) && stat_frame_count > 0 {
                let elapsed = last_stat_time.elapsed().as_secs_f64();
                let fps = frames_since_stat as f64 / elapsed;
                smoothed_fps = if smoothed_fps == 0.0 {
                    fps
                } else {
                    smoothed_fps * 0.7 + fps * 0.3
                };
                let raw_progress = processed_frames as f64 / total_frames as f64;
                let progress = 0.3 + raw_progress * 0.7;
                let eta_secs = (total_frames - processed_frames) as f64 / smoothed_fps;
                let avg_recv = acc_recv_us / stat_frame_count;
                let avg_composite = acc_composite_us / stat_frame_count;
                let avg_preview = acc_preview_us / stat_frame_count;
                let avg_encode = acc_encode_us / stat_frame_count;
                tracing::info!(
                    "视频导出: 帧 {}/{} ({:.0}%), FPS={:.0}, ETA={:.0}s, 阶段耗时(us) recv={} composite={} preview={} encode={}",
                    processed_frames,
                    total_frames,
                    raw_progress * 100.0,
                    smoothed_fps,
                    eta_secs,
                    avg_recv,
                    avg_composite,
                    avg_preview,
                    avg_encode,
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
                    0.0,
                ));
                last_stat_time = Instant::now();
                frames_since_stat = 0;
                acc_recv_us = 0;
                acc_composite_us = 0;
                acc_preview_us = 0;
                acc_encode_us = 0;
                stat_frame_count = 0;
            }
        }
    } // enqueue_frame 在此作用域结束时释放，后续可访问 source

    // drain 剩余 inflight 帧
    while !param_queue.is_empty() && !cancelled {
        let recv_start = Instant::now();
        let data = match frame_rx.recv() {
            Ok(d) => d,
            Err(_) => {
                tracing::error!("drain 阶段帧数据通道关闭");
                cancelled = true;
                break;
            }
        };
        let recv_us = recv_start.elapsed().as_micros() as u64;

        let p = param_queue
            .pop_front()
            .unwrap_or((0.0, 1.0, 60.0, ppq, [0u8; 1024]));
        let (should_stop, stats) = composite_and_encode_frame(
            data,
            p,
            &mut encoder,
            &progress_tx,
            &preview_tx,
            &cancel_flag,
            &mut last_preview_time,
            &mut preview_sent,
            width,
            height,
            &keyboard_pixels,
            kb_w,
            kb_h,
            &recycle_tx,
        );

        acc_recv_us += recv_us;
        acc_composite_us += stats.composite_us;
        acc_preview_us += stats.preview_us;
        acc_encode_us += stats.encode_us;
        stat_frame_count += 1;

        if should_stop {
            cancelled = true;
            break;
        }
        processed_frames += 1;
    }

    // 输出最后一组阶段打点
    if let Some(divisor) = std::num::NonZeroU64::new(stat_frame_count) {
        let d = divisor.get();
        tracing::info!(
            "视频导出: 收尾阶段耗时(us) recv={} composite={} preview={} encode={}",
            acc_recv_us / d,
            acc_composite_us / d,
            acc_preview_us / d,
            acc_encode_us / d,
        );
    }

    let elapsed = start.elapsed().as_secs_f64();
    finalize_video_export(
        encoder,
        cancelled,
        processed_frames,
        elapsed,
        total_frames,
        smoothed_fps,
        &progress_tx,
    );

    // 清理流式 MIDI 缓存文件（先关闭文件句柄再删除）
    let cache_path = source.cache_path().to_path_buf();
    drop(source);
    if let Err(e) = std::fs::remove_file(&cache_path) {
        tracing::warn!("清理 MIDI 缓存文件失败: {e}");
    }
}

/// 发送初始渲染命令：`StartVideoExport`。
///
/// 返回 `true` 表示发生通信错误、调用方应终止后台任务。
fn send_initial_render_commands(
    cmd_sender: &std::sync::mpsc::Sender<lumino_gfx::render_thread::RenderCommand>,
    width: u32,
    height: u32,
    frame_tx: std::sync::mpsc::Sender<Vec<u8>>,
    recycle_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
) -> bool {
    // 发送 StartVideoExport 命令，建立渲染线程对象池回收通道
    if cmd_sender
        .send(RenderCommand::Control(ControlCommand::StartVideoExport {
            width,
            height,
            frame_tx: FrameSender(frame_tx),
            recycle_rx,
        }))
        .is_err()
    {
        tracing::error!("发送 StartVideoExport 命令失败");
        let _ = progress_tx.send(("导出失败：渲染线程通信错误".to_string(), -1.0, 0, 0.0, 0.0));
        return true;
    }

    false
}

/// 单帧处理阶段耗时统计（微秒）
#[derive(Debug, Default)]
struct FrameStageStats {
    /// 键盘 + 标尺合成耗时
    composite_us: u64,
    /// 预览帧克隆/缩放/发送耗时
    preview_us: u64,
    /// ffmpeg 写入耗时
    encode_us: u64,
}

/// 单帧处理：键盘贴图合成 + 标尺数字合成 + 取消检测 + 预览帧发送 + 编码 + 缓冲区归还。
///
/// 返回 `(should_stop, stats)`：`should_stop` 为 true 表示应终止渲染循环（取消或出错）。
#[allow(clippy::too_many_arguments)]
fn composite_and_encode_frame(
    mut data: Vec<u8>,
    params: (f32, f32, f32, u32, [u8; 1024]),
    encoder: &mut FfmpegEncoder,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
    preview_tx: &tokio::sync::mpsc::UnboundedSender<(Vec<u8>, u32, u32)>,
    cancel_flag: &Arc<AtomicBool>,
    last_preview_time: &mut Instant,
    preview_sent: &mut bool,
    width: u32,
    height: u32,
    keyboard_pixels: &[u8],
    kb_w: u32,
    kb_h: u32,
    recycle_tx: &std::sync::mpsc::Sender<Vec<u8>>,
) -> (bool, FrameStageStats) {
    let mut stats = FrameStageStats::default();
    let (sx, zx, kw, ppq_val, key_colors) = params;

    if data.is_empty() {
        tracing::warn!("帧读回为空，跳过");
        return (false, stats);
    }

    let t0 = Instant::now();
    if !keyboard_pixels.is_empty() {
        super::video_export::composite_keyboard(
            &mut data,
            width,
            height,
            keyboard_pixels,
            kb_w,
            kb_h,
            &key_colors,
        );
    }
    super::video_export::composite_ruler_numbers(&mut data, width, height, sx, zx, kw, ppq_val);
    stats.composite_us = t0.elapsed().as_micros() as u64;

    if cancel_flag.load(Ordering::Relaxed) {
        tracing::info!("视频导出：帧数据到达后检测到取消，正在收尾...");
        match encoder.write_frame(data) {
            Ok(frame) => {
                if recycle_tx.send(frame).is_err() {
                    tracing::warn!("取消收尾时帧缓冲区归还失败");
                }
            }
            Err(e) => {
                tracing::error!("取消收尾写入失败: {e}");
                let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
            }
        }
        return (true, stats);
    }

    // 预览帧：在 write_frame（move data）之前 clone 发送。
    // 第一帧立即发送，让预览界面尽快有内容；后续按 200ms 节流。
    if !*preview_sent || last_preview_time.elapsed() >= Duration::from_millis(200) {
        let t0 = Instant::now();
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
            !*preview_sent
        );
        if preview_tx.send((small_data, small_w, small_h)).is_err() {
            tracing::warn!("视频导出: 预览帧发送失败，接收端已关闭");
        }
        *last_preview_time = Instant::now();
        *preview_sent = true;
        stats.preview_us = t0.elapsed().as_micros() as u64;
    }

    let t0 = Instant::now();
    let data = match encoder.write_frame(data) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("写入视频帧失败: {e}");
            let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
            return (true, stats);
        }
    };
    stats.encode_us = t0.elapsed().as_micros() as u64;

    // 将已写入的帧缓冲区归还给渲染线程对象池复用
    if recycle_tx.send(data).is_err() {
        tracing::warn!("帧缓冲区归还失败：回收通道已关闭");
    }

    (false, stats)
}

/// 收尾编码：根据是否取消发送最终进度，并调用 `finish()` 写入文件头。
fn finalize_video_export(
    encoder: FfmpegEncoder,
    cancelled: bool,
    processed_frames: u64,
    elapsed: f64,
    total_frames: u64,
    smoothed_fps: f64,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
) {
    if !cancelled {
        tracing::info!("视频导出完成: 耗时 {:.1}s", elapsed);
        let _ = progress_tx.send((
            "导出完成".to_string(),
            1.0,
            total_frames,
            smoothed_fps,
            elapsed,
        ));
    } else {
        tracing::info!(
            "视频导出取消: 已处理 {} 帧, 耗时 {:.1}s, 正在收尾编码器",
            processed_frames,
            elapsed
        );
        let _ = progress_tx.send((
            "导出已取消".to_string(),
            1.0,
            total_frames,
            smoothed_fps,
            elapsed,
        ));
    }
    if let Err(e) = encoder.finish() {
        tracing::error!("FFmpeg 收尾失败: {e}");
        let _ = progress_tx.send((format!("导出失败: {e}"), -1.0, 0, 0.0, 0.0));
    }
}
