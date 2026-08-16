//! 视频导出窗口事件处理：在后台线程执行逐帧渲染 + FFmpeg 编码，进度通过通道回传主线程。

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use lumino_export::video::{
    FfmpegEncoder, VideoExportConfig,
    config::{Container, EncoderBackend, QualityPreset, VideoCodec},
};
use lumino_gfx::render_thread::{ControlCommand, FrameSender, RenderCommand};

use super::video_export::cli_progress::CliProgressBar;
use super::video_export::streaming::StreamingNoteSource;

/// 单帧合成参数（与帧数据 FIFO 一一对应），替代裸 6 元组避免位置解构出错。
#[derive(Debug, Clone, Copy)]
struct FrameParams {
    /// 标尺滚动偏移（像素）
    scroll_x: f32,
    /// 标尺缩放（像素/tick）
    zoom_x: f32,
    /// 键盘宽度（像素）
    keyboard_width: f32,
    /// 分辨率（Pulses Per Quarter note）
    ppq: u32,
    /// 按键高亮颜色（RGBA × 256 键）
    key_colors: [u8; 1024],
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            zoom_x: 1.0,
            keyboard_width: 60.0,
            ppq: 0,
            key_colors: [0u8; 1024],
        }
    }
}

type EncodeFrameQueue = std::collections::VecDeque<FrameParams>;

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
            render_mode,
            waterfall_scroll_speed,
            miditrail_z_far,
            note_counter,
            data_curve,
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

        // 获取渲染线程命令发送端与表面纹理格式（非 GPU compute 样式使用）
        let main_ui = self.window_state.window.ui();
        let cmd_sender = main_ui.render_command_sender();
        let surface_pix_fmt =
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

        // 用编辑器 tempo_points 覆盖 doc 的加载时原始 tempo（与音频导出/保存一致）：
        // doc.tempo_changes 是加载文件时的值，用户经工程设置/速度面板修改的 BPM
        // 只写入 tempo_points、不回写 doc——不覆盖视频导出的总时长/帧调度/BPM 显示用旧值。
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

        // 创建取消标志
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.window_state.video_export_cancel = cancel_flag.clone();

        // 后台线程：逐帧渲染 + FFmpeg 编码
        let _ = std::thread::Builder::new()
            .name("video-render".into())
            .spawn(move || {
                let is_gpu_compute_style = matches!(
                    render_mode,
                    lumino_event::window::video::RenderMode::Waterfall
                        | lumino_event::window::video::RenderMode::MIDITrail
                );
                // 计数器模式：CPU 端渲染（无卷帘/键盘/标尺），帧数据为 BGRA 直出。
                let is_cpu_renderer = matches!(
                    render_mode,
                    lumino_event::window::video::RenderMode::NoteCounter
                        | lumino_event::window::video::RenderMode::DataCurve
                );
                // GPU compute / 3D 渲染输出为 rgba8unorm storage texture，
                // 因此编码器输入像素格式必须为 "rgba"；
                // 计数器 CPU 渲染直出 BGRA → "bgra"；
                // 其余（NoteRectangle）使用 UI 表面纹理，按表面格式选择。
                let input_pix_fmt = if is_gpu_compute_style {
                    "rgba"
                } else if is_cpu_renderer {
                    "bgra"
                } else {
                    surface_pix_fmt
                };
                // 计数器渲染配置（仅 NoteCounter 模式有效）
                let counter_config = if matches!(
                    render_mode,
                    lumino_event::window::video::RenderMode::NoteCounter
                ) {
                    Some(super::video_export::CounterRenderConfig::from(
                        &note_counter,
                    ))
                } else {
                    None
                };
                // 数据曲线渲染配置（仅 DataCurve 模式有效）
                let data_curve_config = if matches!(
                    render_mode,
                    lumino_event::window::video::RenderMode::DataCurve
                ) {
                    Some(super::video_export::DataCurveRenderConfig::from(
                        &data_curve,
                    ))
                } else {
                    None
                };
                if let Some(document) = document {
                    // Arc 快照不可变：克隆 owned 副本后覆盖 tempo。
                    // ChunkedList 为块级浅拷贝（O(块数) 指针拷贝），不复制音符数据。
                    let mut snapshot = (*document).clone();
                    if !editor_tempos.is_empty() {
                        snapshot.tempo_changes = editor_tempos;
                    }
                    run_video_export_task(
                        config,
                        cmd_sender,
                        progress_tx,
                        preview_tx,
                        Arc::new(snapshot),
                        ppq,
                        fps_f64,
                        key_count,
                        width,
                        height,
                        cancel_flag,
                        input_pix_fmt,
                        is_cpu_renderer,
                        is_gpu_compute_style,
                        waterfall_scroll_speed,
                        miditrail_z_far,
                        render_mode,
                        counter_config,
                        data_curve_config,
                    );
                } else if !midi_path.is_empty() {
                    if is_cpu_renderer {
                        tracing::error!("计数器/数据曲线模式需要完整 MIDI 数据，流式读取不支持");
                        let _ = progress_tx.send((
                            "导出失败：计数器/数据曲线模式需要完整 MIDI 数据，请先加载工程或指定 MIDI 数据源"
                                .to_string(),
                            -1.0,
                            0,
                            0.0,
                            0.0,
                        ));
                    } else {
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
                            surface_pix_fmt,
                        );
                    }
                } else {
                    tracing::error!("视频导出失败：无 MidiDocument 且未指定 MIDI 路径");
                    let _ =
                    send_export_error(&progress_tx, "导出失败：无 MIDI 数据");
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
    is_cpu_renderer: bool,
    is_gpu_compute_style: bool,
    waterfall_scroll_speed: f32,
    miditrail_z_far: f32,
    render_mode: lumino_event::window::video::RenderMode,
    counter_config: Option<super::video_export::CounterRenderConfig>,
    data_curve_config: Option<super::video_export::DataCurveRenderConfig>,
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
            send_export_error(&progress_tx, format!("导出失败: {e}"));
            return;
        }
    };

    // 发送初始渲染命令（StartVideoExport），携带帧缓冲回收通道
    // clone frame_tx：send_initial_render_commands 会消费原始 frame_tx（移至渲染线程），
    // 瀑布流 CPU 路径需在 enqueue_frame 中保留发送能力。
    let frame_tx_waterfall = frame_tx.clone();
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

    // 计数器模式：统计状态 + 字体渲染器 + CSV 写入器
    // 数据曲线模式：统计状态（共用 CounterStats）+ 数据曲线渲染器
    let mut counter_stats: Option<super::video_export::CounterStats> = None;
    let mut counter_renderer: Option<super::video_export::CounterFontRenderer> = None;
    let mut csv_writer: Option<std::io::BufWriter<std::fs::File>> = None;
    let mut data_curve_renderer: Option<super::video_export::DataCurveRenderer> = None;
    // 统计状态：计数器与数据曲线共用同一数据源
    if counter_config.is_some() || data_curve_config.is_some() {
        let mut stats = super::video_export::CounterStats::default();
        stats.reset(&document);
        counter_stats = Some(stats);
    }
    if let Some(cfg) = &counter_config {
        // 字体渲染器：TTF 加载失败时回退内置点阵（导出流程不中断）
        match super::video_export::CounterFontRenderer::new(&cfg.font, cfg.font_size) {
            Ok(r) => {
                tracing::info!("计数器字体加载成功：{}", r.describe());
                counter_renderer = Some(r);
            }
            Err(e) => {
                tracing::warn!("计数器字体加载失败（回退内置点阵）: {e}");
                counter_renderer = Some(
                    super::video_export::CounterFontRenderer::new(
                        &lumino_event::window::video::CounterFont::Bitmap,
                        cfg.font_size,
                    )
                    .expect("内置点阵字体渲染器不会失败"),
                );
            }
        }
        if cfg.save_csv && !cfg.csv_output.as_os_str().is_empty() {
            match std::fs::File::create(&cfg.csv_output) {
                Ok(f) => csv_writer = Some(std::io::BufWriter::new(f)),
                Err(e) => tracing::warn!("计数器 CSV 文件创建失败: {e}"),
            }
        }
    }
    if let Some(cfg) = &data_curve_config {
        let fps_u32 = fps_f64.max(1.0) as u32;
        match super::video_export::DataCurveRenderer::new(cfg, fps_u32) {
            Ok(r) => {
                tracing::info!("数据曲线渲染器就绪（窗口 {} 帧）", r.window_cap());
                data_curve_renderer = Some(r);
            }
            Err(e) => {
                tracing::warn!("数据曲线字体加载失败（回退内置点阵）: {e}");
                let fallback = super::video_export::DataCurveRenderConfig {
                    font: lumino_event::window::video::CounterFont::Bitmap,
                    ..cfg.clone()
                };
                data_curve_renderer = Some(
                    super::video_export::DataCurveRenderer::new(&fallback, fps_u32)
                        .expect("内置点阵字体渲染器不会失败"),
                );
            }
        }
    }

    let mut render_bar = CliProgressBar::new(30, "视频渲染");
    render_bar.update(
        0.0,
        &format!(
            "总时长 {:.1}s | 总帧数 {} | PPQ {}",
            duration_secs, total_frames, ppq
        ),
    );

    let mut last_preview_time = Instant::now();
    let mut preview_sent = false;

    // ★ 生成键盘贴图（使用 CPU 贴图方式，在帧数据上合成）
    let (keyboard_pixels, kb_w, kb_h) =
        super::video_export::generate_keyboard_texture(width, height, key_count);

    // 流水线渲染：Runner 预填充 4 帧命令，让 staging ring 从开始就满载，
    // 之后每处理完一帧立即补发下一帧，保持 GPU/CPU 流水线持续运转。
    // 每帧参数携带该帧的按键高亮颜色（RGBAx256 键），用于后台线程合成键盘。
    let mut param_queue: EncodeFrameQueue = EncodeFrameQueue::with_capacity(16);

    // 复用缓冲区避免每帧堆分配
    let mut visible_note_buf: Vec<super::video_export::SortableNote> = Vec::with_capacity(4096);
    let mut note_instances_buf: Vec<lumino_gfx::NoteInstance> = Vec::with_capacity(4096);

    // 闭包不捕获 param_queue，而是作为参数传入，避免与主循环中的 pop_front 产生可变借用冲突。
    let mut enqueue_frame = |queue: &mut EncodeFrameQueue, frame_idx: u64| -> bool {
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
        queue.push_back(FrameParams {
            scroll_x: video_scroll_x,
            zoom_x: video_zoom_x,
            keyboard_width: video_kb_width,
            ppq,
            key_colors,
        });

        // 瀑布流/计数器模式（CPU 端渲染）：绕过 GPU compute shader + readback 开销
        // 参考 Zenith-MIDI 和 fmr 的视频导出策略——CPU 渲染直出 BGRA，无需 GPU 管线参与。
        // waterfall.wgsl compute shader 每像素扫描所有音符(O(notes×pixels))，
        // GPU→CPU 回读(staging buffer)引入额外延迟，而 CPU 路径仅需 O(visible_notes)。
        if is_cpu_renderer {
            let mut frame_data = vec![0u8; (width as usize) * (height as usize) * 4];
            use lumino_event::window::video::RenderMode;
            match render_mode {
                RenderMode::Waterfall => {
                    super::video_export::render_waterfall_frame(
                        &mut frame_data,
                        width,
                        height,
                        &document,
                        tick,
                        ppq,
                        key_count,
                        waterfall_scroll_speed,
                    );
                }
                RenderMode::NoteCounter => {
                    // 计数器模式：统计推进 + 文本模板渲染（无卷帘/键盘/标尺）
                    let cfg = counter_config.as_ref().expect("计数器模式必须有渲染配置");
                    let stats = counter_stats.as_mut().expect("计数器模式必须有统计状态");
                    let renderer = counter_renderer
                        .as_mut()
                        .expect("计数器模式必须有字体渲染器");
                    let out = super::video_export::render_counter_frame(
                        &mut frame_data,
                        width,
                        height,
                        &document,
                        tick,
                        ppq,
                        fps_f64 as u32,
                        duration_secs,
                        cfg,
                        stats,
                        renderer,
                    );
                    // CSV 行写入（失败仅告警，不中断渲染）
                    if let (Some(line), Some(writer)) = (out.csv_line, csv_writer.as_mut()) {
                        use std::io::Write;
                        if let Err(e) = writeln!(writer, "{line}") {
                            tracing::warn!("计数器 CSV 写入失败: {e}");
                        }
                    }
                    // 首帧诊断：确认模板渲染内容（与流式模式诊断风格一致）
                    static COUNTER_DIAG: std::sync::atomic::AtomicU32 =
                        std::sync::atomic::AtomicU32::new(0);
                    let diag_idx = COUNTER_DIAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if diag_idx < 3 {
                        tracing::info!(
                            "计数器模式诊断[{diag_idx}]: tick={tick} stats=({},poly {},nps {}) text=\"{}\"",
                            stats.note_count,
                            stats.polyphony,
                            stats.nps,
                            out.text.replace('\n', "\\n"),
                        );
                    }
                }
                RenderMode::NoteRectangle => unreachable!("NoteRectangle 应走 GPU 路径"),
                RenderMode::MIDITrail => unreachable!("MIDITrail 应走 GPU 3D 路径"),
                RenderMode::DataCurve => {
                    // 数据曲线模式：统计推进 → 取指标值 → 环形窗口 → 帧渲染
                    let cfg = data_curve_config
                        .as_ref()
                        .expect("数据曲线模式必须有渲染配置");
                    let stats = counter_stats.as_mut().expect("数据曲线模式必须有统计状态");
                    let renderer = data_curve_renderer
                        .as_mut()
                        .expect("数据曲线模式必须有渲染器");
                    // 关键：推进统计到当前 tick（与计数器分支一致）。
                    // 缺失此调用会导致 NPS/复音数/音符数永远停留在 0 → 曲线为 0 值直线。
                    stats.advance(&document, tick, fps_f64 as u32);
                    let value = match cfg.metric {
                        lumino_event::window::video::DataCurveMetric::Nps => stats.nps as f64,
                        lumino_event::window::video::DataCurveMetric::Polyphony => {
                            stats.polyphony as f64
                        }
                        lumino_event::window::video::DataCurveMetric::NoteCount => {
                            stats.note_count as f64
                        }
                        lumino_event::window::video::DataCurveMetric::Bpm => {
                            super::video_export::current_bpm(&document.tempo_changes, tick)
                        }
                    };
                    renderer.push_value(value);
                    let out = super::video_export::render_data_curve_frame(
                        &mut frame_data,
                        width,
                        height,
                        renderer,
                        cfg,
                    );
                    // 首帧诊断：确认指标值与缩放状态
                    static DATA_CURVE_DIAG: std::sync::atomic::AtomicU32 =
                        std::sync::atomic::AtomicU32::new(0);
                    let diag_idx =
                        DATA_CURVE_DIAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if diag_idx < 3 {
                        tracing::info!(
                            "数据曲线模式诊断[{diag_idx}]: tick={tick} value={} zoom=({}, {})",
                            out.value,
                            out.min,
                            out.max,
                        );
                    }
                }
            }

            // 帧数据直接送入编码通道，跳过渲染线程的 GPU 路径
            if frame_tx_waterfall.send(frame_data).is_err() {
                tracing::error!("CPU 渲染帧发送失败：通道已关闭");
                let _ =
                    send_export_error(&progress_tx, "导出失败：帧通道通信错误");
                return true;
            }
        } else {
            let params = super::video_export::build_video_export_render_params(
                width,
                height,
                tick,
                &document,
                ppq,
                key_count,
                render_mode,
                waterfall_scroll_speed,
                miditrail_z_far,
                fps_f64 as f32,
                &mut visible_note_buf,
                &mut note_instances_buf,
            );

            if cmd_sender
                .send(RenderCommand::Control(ControlCommand::RenderVideoFrame {
                    params: Box::new(params),
                }))
                .is_err()
            {
                tracing::error!("发送 RenderVideoFrame 命令失败");
                let _ =
                    send_export_error(&progress_tx, "导出失败：渲染线程通信错误");
                return true;
            }
        }
        false
    };

    // 预填充 + 主循环 + drain 由公共 FramePipeline 驱动（与流式路径共用同一循环骨架）
    let mut pipeline = FramePipeline {
        total_frames,
        cancel_flag: &cancel_flag,
        frame_rx: &frame_rx,
        param_queue: &mut param_queue,
        progress_tx: &progress_tx,
        render_bar: &mut render_bar,
        start,
        // 内存路径：进度直接映射（无解析阶段）
        progress_map: |p| p,
    };
    let mut process_frame = |frame_data: Vec<u8>, frame_params: FrameParams| {
        if is_gpu_compute_style || is_cpu_renderer {
            // GPU compute（瀑布流/MIDITrail）帧已完整渲染；
            // CPU 渲染（计数器）帧也不含键盘/标尺，均直接编码。
            composite_and_encode_frame(
                frame_data,
                FrameParams::default(),
                &mut encoder,
                &progress_tx,
                &preview_tx,
                &cancel_flag,
                &mut last_preview_time,
                &mut preview_sent,
                width,
                height,
                &[],
                0,
                0,
                &recycle_tx,
            )
        } else {
            composite_and_encode_frame(
                frame_data,
                frame_params,
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
            )
        }
    };
    let (processed_frames, cancelled, smoothed_fps) =
        pipeline.run(&mut enqueue_frame, &mut process_frame);
    // enqueue_frame 在此处释放（持有 csv_writer 可变借用），随后可 flush 计数器 CSV。

    // 完成编码：无论是否取消都必须调用 finish()，
    // 否则 FFmpeg 收不到 EOF，视频文件头未写入导致损坏。
    // 用户取消时已写入的帧仍可生成可播放的部分视频。
    let elapsed = start.elapsed().as_secs_f64();
    if let Some(mut writer) = csv_writer {
        use std::io::Write;
        if let Err(e) = writer.flush() {
            tracing::warn!("计数器 CSV 收尾失败: {e}");
        }
    }
    if cancelled {
        render_bar.finish(&format!(
            "已取消 | 已处理 {}/{} 帧 | 耗时 {:.1}s",
            processed_frames, total_frames, elapsed
        ));
    } else {
        render_bar.finish(&format!(
            "完成 {}/{} 帧 | 耗时 {:.1}s",
            processed_frames, total_frames, elapsed
        ));
    }
    finalize_video_export(
        encoder,
        cancelled,
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

    // 阶段 1：解析 MIDI → 硬盘缓存（终端进度条）
    let parse_bar = Arc::new(Mutex::new(CliProgressBar::new(30, "MIDI解析")));
    let progress_tx_for_parse = progress_tx.clone();
    let parse_bar_for_cb = parse_bar.clone();
    let parse_progress: std::sync::Arc<dyn Fn(String, f64) + Send + Sync> =
        std::sync::Arc::new(move |message: String, value: f64| {
            if let Ok(mut bar) = parse_bar_for_cb.lock() {
                bar.update(value, &message);
            }
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
            send_export_error(&progress_tx, format!("导出失败: {e}"));
            return;
        }
    };
    if let Ok(mut bar) = parse_bar.lock() {
        bar.finish("缓存就绪");
    }

    let mut source = match StreamingNoteSource::open(streaming_result) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("视频导出打开流式数据源失败: {e}");
            send_export_error(&progress_tx, format!("导出失败: {e}"));
            return;
        }
    };

    let ppq = source.ppqn();
    let total_frames = source.total_frames();
    let total_ticks = source.total_ticks();
    let duration_secs = source.compute_duration_secs();

    let mut render_bar = CliProgressBar::new(30, "视频渲染");
    render_bar.update(
        0.0,
        &format!(
            "总时长 {:.1}s | 总帧数 {} | PPQN {} | total_ticks {}",
            duration_secs, total_frames, ppq, total_ticks
        ),
    );

    // 创建帧数据通道与回收通道
    let (frame_tx, frame_rx) = channel::<Vec<u8>>();
    let (recycle_tx, recycle_rx) = channel::<Vec<u8>>();

    // 创建 FFmpeg 编码器
    let mut encoder = match FfmpegEncoder::new(&config, input_pix_fmt) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("FFmpeg 创建失败: {e}");
            send_export_error(&progress_tx, format!("导出失败: {e}"));
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

    let mut last_preview_time = Instant::now();
    let mut preview_sent = false;

    let mut param_queue: EncodeFrameQueue = EncodeFrameQueue::new();

    // 入队闭包：读取流式音符、计算键色、发送渲染命令
    let (processed_frames, cancelled, smoothed_fps) = {
        let mut enqueue_frame =
            |queue: &mut EncodeFrameQueue,
             frame_idx: u64|
             -> bool {
                let (notes, params) = match source
                    .read_notes_and_params_for_frame(frame_idx, width, height, fps_f64)
                {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!("读取流式音符失败: {e}");
                        send_export_error(&progress_tx, format!("导出失败: {e}"));
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

                queue.push_back(FrameParams {
                    scroll_x: video_scroll_x,
                    zoom_x: video_zoom_x,
                    keyboard_width: video_kb_width,
                    ppq,
                    key_colors,
                });

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

        // 预填充 + 主循环 + drain 由公共 FramePipeline 驱动（与内存路径共用同一循环骨架）
        let mut pipeline = FramePipeline {
                total_frames,
                cancel_flag: &cancel_flag,
                frame_rx: &frame_rx,
                param_queue: &mut param_queue,
                progress_tx: &progress_tx,
                render_bar: &mut render_bar,
                start,
                // 流式路径：解析阶段进度 0-0.3，渲染阶段映射到 0.3-1.0
                progress_map: |raw| 0.3 + raw * 0.7,
            };
            let mut process_frame = |stream_frame: Vec<u8>, stream_params: FrameParams| {
                composite_and_encode_frame(
                    stream_frame,
                    stream_params,
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
                )
            };
            pipeline.run(&mut enqueue_frame, &mut process_frame)
    }; // 块结束：enqueue_frame/process_frame/pipeline 释放，后续可访问 source

    let elapsed = start.elapsed().as_secs_f64();
    if cancelled {
        render_bar.finish(&format!(
            "已取消 | 已处理 {}/{} 帧 | 耗时 {:.1}s",
            processed_frames, total_frames, elapsed
        ));
    } else {
        render_bar.finish(&format!(
            "完成 {}/{} 帧 | 耗时 {:.1}s",
            processed_frames, total_frames, elapsed
        ));
    }
    finalize_video_export(
        encoder,
        cancelled,
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

/// 发送导出失败进度消息（progress=-1 表示失败，UI 据此弹出错误）。
///
/// 收敛各处重复的 5 元组 `("导出失败: ..", -1.0, 0, 0.0, 0.0)` 发送。
fn send_export_error(
    progress_tx: &tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
    message: impl Into<String>,
) {
    let _ = progress_tx.send((message.into(), -1.0, 0, 0.0, 0.0));
}

/// 帧流水线循环参数（预填充 → 主循环 → drain），内存/流式两条导出路径共用。
struct FramePipeline<'a> {
    total_frames: u64,
    cancel_flag: &'a AtomicBool,
    frame_rx: &'a std::sync::mpsc::Receiver<Vec<u8>>,
    param_queue: &'a mut EncodeFrameQueue,
    progress_tx: &'a tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
    render_bar: &'a mut CliProgressBar,
    /// 墙钟起点（用于进度消息中的"已用时间"）
    start: Instant,
    /// 渲染进度映射：内存路径恒等，流式路径 `0.3 + raw * 0.7`（解析阶段占 0-0.3）
    progress_map: fn(f64) -> f64,
}

impl<'a> FramePipeline<'a> {
    /// 运行完整流水线：预填充 PIPELINE_DEPTH 帧 → 主循环（收帧→处理→补发）→ drain 余帧。
    ///
    /// - `enqueue(queue, frame_idx)`：入队并发送第 frame_idx 帧；返回 true 表示应终止。
    ///   queue 由本方法传入，闭包不得自行捕获 param_queue（避免双重可变借用）。
    /// - `process(frame_data, frame_params)`：处理单帧；返回 `(should_stop, stats)`。
    ///
    /// 返回 `(processed_frames, cancelled, smoothed_fps)`。
    fn run<FE, FP>(&mut self, mut enqueue: FE, mut process: FP) -> (u64, bool, f64)
    where
        FE: FnMut(&mut EncodeFrameQueue, u64) -> bool,
        FP: FnMut(Vec<u8>, FrameParams) -> (bool, FrameStageStats),
    {
        const PIPELINE_DEPTH: usize = 4;
        let total_frames = self.total_frames;
        let mut processed_frames = 0u64;
        let mut cancelled = false;
        let mut next_frame_to_send = 0u64;

        let mut last_stat_time = Instant::now();
        let mut frames_since_stat = 0u64;
        let mut smoothed_fps = 0.0f64;
        let mut acc_recv_us = 0u64;
        let mut acc_composite_us = 0u64;
        let mut acc_preview_us = 0u64;
        let mut acc_encode_us = 0u64;
        let mut stat_frame_count = 0u64;

        // 预填充 inflight，让 GPU 从第一帧就进入流水线满载状态
        for _ in 0..PIPELINE_DEPTH.min(total_frames as usize) {
            if self.cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("视频导出：用户取消，正在收尾...");
                cancelled = true;
                break;
            }
            if enqueue(self.param_queue, next_frame_to_send) {
                cancelled = true;
                break;
            }
            next_frame_to_send += 1;
        }

        // 主循环：每收到一帧就合成/编码，并立即补发下一帧命令
        while processed_frames < total_frames && !cancelled {
            if self.cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("视频导出：用户取消，正在收尾...");
                cancelled = true;
                break;
            }

            let recv_start = Instant::now();
            let frame_data = match self.frame_rx.recv() {
                Ok(buf) => buf,
                Err(_) => {
                    tracing::error!("帧数据通道关闭");
                    send_export_error(self.progress_tx, "导出失败：帧数据通道关闭");
                    cancelled = true;
                    break;
                }
            };
            let recv_us = recv_start.elapsed().as_micros() as u64;

            // 默认值仅在 queue 与帧数据 FIFO 失步时出现（理论不发生），ppq 用 0 无实际影响
            let frame_params = self
                .param_queue
                .pop_front()
                .unwrap_or(FrameParams::default());
            let (should_stop, stats) = process(frame_data, frame_params);

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
                if enqueue(self.param_queue, next_frame_to_send) {
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
                let progress = (self.progress_map)(raw_progress);
                let eta_secs = (total_frames - processed_frames) as f64 / smoothed_fps;
                let avg_recv = acc_recv_us / stat_frame_count;
                let avg_composite = acc_composite_us / stat_frame_count;
                let avg_preview = acc_preview_us / stat_frame_count;
                let avg_encode = acc_encode_us / stat_frame_count;
                self.render_bar.update(
                    raw_progress,
                    &format!(
                        "帧 {}/{} | FPS {:.0} | ETA {:.0}s | recv={} composite={} preview={} encode={}",
                        processed_frames,
                        total_frames,
                        smoothed_fps,
                        eta_secs,
                        avg_recv,
                        avg_composite,
                        avg_preview,
                        avg_encode,
                    ),
                );
                let _ = self.progress_tx.send((
                    format!(
                        "{:.0}% | FPS {:.0} | ETA {:.0}s",
                        progress * 100.0,
                        smoothed_fps,
                        eta_secs
                    ),
                    progress,
                    total_frames,
                    smoothed_fps,
                    // 真实已用时间（墙钟），供 UI 显示"已用时间"
                    self.start.elapsed().as_secs_f64(),
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
        while !self.param_queue.is_empty() && !cancelled {
            let drain_frame = match self.frame_rx.recv() {
                Ok(buf) => buf,
                Err(_) => {
                    tracing::error!("drain 阶段帧数据通道关闭");
                    cancelled = true;
                    break;
                }
            };

            let drain_params = self
                .param_queue
                .pop_front()
                .unwrap_or(FrameParams::default());
            let (should_stop, _stats) = process(drain_frame, drain_params);

            if should_stop {
                cancelled = true;
                break;
            }
            processed_frames += 1;
        }

        (processed_frames, cancelled, smoothed_fps)
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
        send_export_error(progress_tx, "导出失败：渲染线程通信错误");
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
/// 瀑布流/MIDITrail/计数器模式帧已由 GPU/CPU 完整渲染，调用时传
/// `FrameParams::default()` + 空键盘贴图即可跳过合成（与旧 `composite_waterfall_and_encode_frame` 等价）。
///
/// 返回 `(should_stop, stats)`：`should_stop` 为 true 表示应终止渲染循环（取消或出错）。
#[allow(clippy::too_many_arguments)]
fn composite_and_encode_frame(
    mut data: Vec<u8>,
    params: FrameParams,
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
    let FrameParams {
        scroll_x: sx,
        zoom_x: zx,
        keyboard_width: kw,
        ppq: ppq_val,
        key_colors,
        ..
    } = params;

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
        super::video_export::composite_ruler_numbers(&mut data, width, height, sx, zx, kw, ppq_val);
    }
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
                send_export_error(&progress_tx, format!("导出失败: {e}"));
            }
        }
        return (true, stats);
    }

    // 预览帧：在 write_frame（move data）之前生成。
    // 第一帧立即发送，让预览界面尽快有内容；后续按 200ms 节流。
    // 使用 downscale_bgra_to_rgba 合并 BGRA→RGBA 交换与缩小为单次遍历，
    // 避免全帧 clone（~8MB@1080p）以节省内存带宽。
    if !*preview_sent || last_preview_time.elapsed() >= Duration::from_millis(200) {
        let t0 = Instant::now();
        // GPU 读回是 BGRA 格式，但 image::Handle::from_rgba 需要 RGBA
        const PREVIEW_MAX_W: u32 = 480;
        let (small_data, small_w, small_h) = if width > PREVIEW_MAX_W {
            let scale = PREVIEW_MAX_W as f64 / width as f64;
            let tw = PREVIEW_MAX_W;
            let th = (height as f64 * scale).round() as u32;
            // 单次分配 + 缩放 + BGR 交换，零额外 clone
            super::downscale_bgra_to_rgba(&data, width, height, tw, th)
        } else {
            // 不需要缩小：clone 并在 clone 上做 BGR 交换
            let mut preview_data = data.clone();
            for pixel in preview_data.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            (preview_data, width, height)
        };

        if preview_tx.send((small_data, small_w, small_h)).is_err() {
            tracing::warn!("视频导出: 预览帧发送失败，接收端已关闭");
        }
        *last_preview_time = Instant::now();
        *preview_sent = true;
        stats.preview_us = t0.elapsed().as_micros() as u64;
    }

    let t0 = Instant::now();
    let encoded_frame = match encoder.write_frame(data) {
        Ok(buf) => buf,
        Err(e) => {
            tracing::error!("写入视频帧失败: {e}");
            send_export_error(&progress_tx, format!("导出失败: {e}"));
            return (true, stats);
        }
    };
    stats.encode_us = t0.elapsed().as_micros() as u64;

    // 将已写入的帧缓冲区归还给渲染线程对象池复用
    if recycle_tx.send(encoded_frame).is_err() {
        tracing::warn!("帧缓冲区归还失败：回收通道已关闭");
    }

    (false, stats)
}

/// 收尾编码：根据是否取消发送最终进度，并调用 `finish()` 写入文件头。
fn finalize_video_export(
    encoder: FfmpegEncoder,
    cancelled: bool,
    elapsed: f64,
    total_frames: u64,
    smoothed_fps: f64,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<(String, f64, u64, f64, f64)>,
) {
    if !cancelled {
        let _ = progress_tx.send((
            "导出完成".to_string(),
            1.0,
            total_frames,
            smoothed_fps,
            elapsed,
        ));
    } else {
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
        send_export_error(progress_tx, format!("导出失败: {e}"));
    }
}
