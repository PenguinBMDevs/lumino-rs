use super::enqueue::enqueue_memory_frame;
use super::*;

/// 后台线程主流程：创建编码器、发送初始渲染命令、逐帧渲染 + 编码、收尾。
///
/// 该函数整体等价于原 `handle_start_video_export` 中 `move` 闭包体内的逻辑，
/// 仅将各阶段进一步拆分成下方私有步骤函数，行为保持一致。
pub(crate) struct RunVideoExportTaskInput {
    pub config: VideoExportConfig,
    pub cmd_sender: Sender<RenderCommand>,
    pub progress_tx: UnboundedSender<ProgressMsg>,
    pub preview_tx: UnboundedSender<(Vec<u8>, u32, u32)>,
    pub document: Arc<lumino_midi_loader::MidiDocument>,
    pub ppq: u32,
    pub fps_f64: f64,
    pub key_count: u16,
    pub width: u32,
    pub height: u32,
    pub cancel_flag: Arc<AtomicBool>,
    pub input_pix_fmt: &'static str,
    pub is_cpu_renderer: bool,
    pub is_gpu_compute_style: bool,
    pub waterfall_scroll_speed: f32,
    pub miditrail_z_far: f32,
    pub miditrail_view_mode: MiditrailViewMode,
    pub miditrail_normal_speed: f32,
    pub miditrail_top_speed: f32,
    pub miditrail_3d_notes: bool,
    pub render_mode: RenderMode,
    pub counter_config: Option<CounterRenderConfig>,
    pub data_curve_config: Option<DataCurveRenderConfig>,
    pub midi_console_config: Option<MidiConsoleRenderConfig>,
}

pub(crate) fn run_video_export_task(input: RunVideoExportTaskInput) {
    let RunVideoExportTaskInput {
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
        is_cpu_renderer,
        is_gpu_compute_style,
        waterfall_scroll_speed,
        miditrail_z_far,
        miditrail_view_mode,
        miditrail_normal_speed,
        miditrail_top_speed,
        miditrail_3d_notes,
        render_mode,
        counter_config,
        data_curve_config,
        midi_console_config,
        ..
    } = input;
    let start = Instant::now();

    // 按键颜色增量扫描状态（与编辑器 PlaybackScanState 等价）
    let mut key_color_state = keyboard::PlaybackKeyColorState::default();
    let mut key_colors = [0u8; keyboard::KEY_COLOR_BYTES];

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
    let duration_secs = video_export::compute_duration_secs(tempo_changes, total_ticks, ppq);
    let total_frames = config.total_frames(duration_secs);

    // 计数器模式：统计状态 + 字体渲染器 + CSV 写入器
    // 数据曲线模式：统计状态（共用 CounterStats）+ 数据曲线渲染器
    let mut counter_stats: Option<CounterStats> = None;
    let mut counter_renderer: Option<CounterFontRenderer> = None;
    let mut csv_writer: Option<std::io::BufWriter<std::fs::File>> = None;
    let mut data_curve_renderer: Option<DataCurveRenderer> = None;
    // 统计状态：计数器与数据曲线共用同一数据源
    if counter_config.is_some() || data_curve_config.is_some() {
        let mut stats = CounterStats::default();
        stats.reset(&document);
        counter_stats = Some(stats);
    }
    if let Some(cfg) = &counter_config {
        // 字体渲染器：TTF 加载失败时回退内置点阵（导出流程不中断）
        match CounterFontRenderer::new(&cfg.font, cfg.font_size) {
            Ok(r) => {
                tracing::info!("计数器字体加载成功：{}", r.describe());
                counter_renderer = Some(r);
            }
            Err(e) => {
                tracing::warn!("计数器字体加载失败（回退内置点阵）: {e}");
                counter_renderer = match CounterFontRenderer::new(
                    &lumino_message::events::window::video::CounterFont::Bitmap,
                    cfg.font_size,
                ) {
                    Ok(r) => Some(r),
                    Err(fallback_e) => {
                        // 内置点阵理论上不会失败；若异常（资源/渲染初始化问题），
                        // 降级为不渲染计数器，避免导出任务崩溃。
                        tracing::error!("内置点阵字体渲染器也失败（计数器将不渲染）: {fallback_e}");
                        None
                    }
                };
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
        match DataCurveRenderer::new(cfg, fps_u32) {
            Ok(r) => {
                tracing::info!("数据曲线渲染器就绪（窗口 {} 帧）", r.window_cap());
                data_curve_renderer = Some(r);
            }
            Err(e) => {
                tracing::warn!("数据曲线字体加载失败（回退内置点阵）: {e}");
                let fallback = DataCurveRenderConfig {
                    font: lumino_message::events::window::video::CounterFont::Bitmap,
                    ..cfg.clone()
                };
                data_curve_renderer = match DataCurveRenderer::new(&fallback, fps_u32) {
                    Ok(r) => Some(r),
                    Err(fallback_e) => {
                        // 内置点阵理论上不会失败；异常时降级为不渲染数据曲线。
                        tracing::error!(
                            "内置点阵数据曲线渲染器也失败（数据曲线将不渲染）: {fallback_e}"
                        );
                        None
                    }
                };
            }
        }
    }

    // MidiConsole 风格渲染器（全文档模式，需完整 MIDI 数据）
    let mut midi_console_renderer: Option<MidiConsoleRenderer> = None;
    if let Some(cfg) = &midi_console_config {
        midi_console_renderer = Some(MidiConsoleRenderer::new(&document, cfg));
    }

    let mut render_bar = video_export::cli_progress::CliProgressBar::new(30, "视频渲染");
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
        video_export::generate_keyboard_texture(width, height, key_count);

    // 流水线渲染：Runner 预填充 4 帧命令，让 staging ring 从开始就满载，
    // 之后每处理完一帧立即补发下一帧，保持 GPU/CPU 流水线持续运转。
    // 每帧参数携带该帧的按键高亮颜色（RGBAx256 键），用于后台线程合成键盘。
    let mut param_queue: EncodeFrameQueue = EncodeFrameQueue::with_capacity(16);

    // 复用缓冲区避免每帧堆分配
    let mut visible_note_buf: Vec<SortableNote> = Vec::with_capacity(4096);
    let mut note_instances_buf: Vec<lumino_gfx::NoteInstance> = Vec::with_capacity(4096);
    // 首帧全量上传标记（GPU 常驻后后续帧只发 uniforms）
    let mut notes_uploaded = false;
    // 窗口收集滑动状态（瀑布流/MIDITrail 同一任务内复用）
    let mut window_state = video_export::WindowCollectState::default();

    // 闭包不捕获 param_queue，而是作为参数传入，避免与主循环中的 pop_front 产生可变借用冲突。
    let mut ctx = MemoryEnqueueCtx {
        cmd_sender: &cmd_sender,
        document: &document,
        fps_f64,
        ppq,
        width,
        height,
        key_count,
        is_cpu_renderer,
        is_gpu_compute_style,
        render_mode,
        counter_config: &counter_config,
        counter_stats: &mut counter_stats,
        counter_renderer: &mut counter_renderer,
        data_curve_config: &data_curve_config,
        data_curve_renderer: &mut data_curve_renderer,
        midi_console_config: &midi_console_config,
        midi_console_renderer: &mut midi_console_renderer,
        duration_secs,
        waterfall_scroll_speed,
        miditrail_z_far,
        miditrail_view_mode,
        miditrail_normal_speed,
        miditrail_top_speed,
        miditrail_3d_notes,
        frame_tx_waterfall: &frame_tx_waterfall,
        progress_tx: &progress_tx,
        key_colors: &mut key_colors,
        key_color_state: &mut key_color_state,
        csv_writer: &mut csv_writer,
        visible_note_buf: &mut visible_note_buf,
        note_instances_buf: &mut note_instances_buf,
        notes_uploaded: &mut notes_uploaded,
        window_state: &mut window_state,
    };
    let mut enqueue_frame = |queue: &mut EncodeFrameQueue, frame_idx: u64| -> bool {
        enqueue_memory_frame(&mut ctx, queue, frame_idx)
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
            composite_and_encode_frame(CompositeEncodeFrameInput {
                data: frame_data,
                params: FrameParams::default(),
                encoder: &mut encoder,
                progress_tx: &progress_tx,
                preview_tx: &preview_tx,
                cancel_flag: &cancel_flag,
                last_preview_time: &mut last_preview_time,
                preview_sent: &mut preview_sent,
                width,
                height,
                keyboard_pixels: &[],
                kb_w: 0,
                kb_h: 0,
                recycle_tx: &recycle_tx,
            })
        } else {
            composite_and_encode_frame(CompositeEncodeFrameInput {
                data: frame_data,
                params: frame_params,
                encoder: &mut encoder,
                progress_tx: &progress_tx,
                preview_tx: &preview_tx,
                cancel_flag: &cancel_flag,
                last_preview_time: &mut last_preview_time,
                preview_sent: &mut preview_sent,
                width,
                height,
                keyboard_pixels: &keyboard_pixels,
                kb_w,
                kb_h,
                recycle_tx: &recycle_tx,
            })
        }
    };
    let (processed_frames, cancelled, smoothed_fps) =
        pipeline.run(&mut enqueue_frame, &mut process_frame);
    // enqueue_frame 在此处释放（持有 csv_writer 可变借用），随后可 flush 计数器 CSV。

    // 完成编码：无论是否取消都必须调用 finish()，
    // 否则 FFmpeg 收不到 EOF，视频文件头未写入导致损坏。
    // 用户取消时已写入的帧仍可生成可播放的部分视频。
    let elapsed = start.elapsed().as_secs_f64();
    if let Some(writer) = ctx.csv_writer.as_mut() {
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
        &cmd_sender,
        encoder,
        cancelled,
        elapsed,
        total_frames,
        smoothed_fps,
        &progress_tx,
    );
}
