use super::*;

/// 内存模式单帧入队（原 `run_video_export_task` 内嵌 `enqueue_frame` 闭包抽出的自由函数）。
///
/// 计算当前 tick 的按键高亮色与标尺偏移并入队 `FrameParams`；CPU 渲染模式
/// （瀑布流/计数器/数据曲线）在此直接生成帧数据并送入编码通道，跳过 GPU 路径。
/// 返回 `true` 表示应终止渲染循环（配置缺失/通道关闭/取消）。
pub(super) fn enqueue_memory_frame(
    ctx: &mut MemoryEnqueueCtx,
    queue: &mut EncodeFrameQueue,
    frame_idx: u64,
) -> bool {
    let time_sec = frame_idx as f64 / ctx.fps_f64;
    let tempo_changes = &ctx.document.tempo_changes;
    let tick = video_export::seconds_to_tick(time_sec, tempo_changes, ctx.ppq);

    // 根据当前播放 tick 增量计算按键高亮颜色（仅钢琴卷帘合成路径消费）。
    // GPU compute（瀑布流/MIDITrail）与 CPU 渲染（计数器等）走 FrameParams::default()
    // + 空键盘贴图，key_colors 无人读取——跳过整次增量扫描（含每轨 O(前缀) skip），
    // 高数据量下这是每帧数十毫秒的死工作（见 recv=20~63ms 根因）。
    if !ctx.is_cpu_renderer && !ctx.is_gpu_compute_style {
        video_export::keyboard::update_playback_key_colors(
            ctx.document,
            tick,
            ctx.key_color_state,
            ctx.key_colors,
        );
    }

    // 计算 scroll_x / zoom_x，用于标尺小节号合成
    let video_kb_width = 60.0f32;
    let video_viewport_tick_span = (ctx.ppq * 16).max(1) as f32;
    let video_zoom_x = (ctx.width as f32 - video_kb_width) / video_viewport_tick_span;
    let video_scroll_x = tick as f32 * video_zoom_x;

    // 入队帧合成参数（与帧数据 FIFO 对应）
    queue.push_back(FrameParams {
        scroll_x: video_scroll_x,
        zoom_x: video_zoom_x,
        keyboard_width: video_kb_width,
        ppq: ctx.ppq,
        key_colors: *ctx.key_colors,
    });

    // 计数器/数据曲线/MidiConsole 模式（CPU 端渲染）：绕过 GPU 开销，BGRA 直出。
    // 注：瀑布流走 GPU compute 管线（见 RenderMode::Waterfall 的 GPU 分支），此处无 CPU 分支。
    if ctx.is_cpu_renderer {
        let mut frame_data = vec![0u8; (ctx.width as usize) * (ctx.height as usize) * 4];
        match ctx.render_mode {
            RenderMode::Waterfall => {
                send_export_error(
                    ctx.progress_tx,
                    "导出失败：Waterfall 模式不应进入 CPU 渲染分支（内部错误）",
                );
                return true;
            }
            RenderMode::NoteCounter => {
                // 计数器模式：统计推进 + 文本模板渲染（无卷帘/键盘/标尺）
                // 配置缺失 = 内部状态不一致：优雅终止导出，不 panic 渲染线程
                let Some(cfg) = ctx.counter_config.as_ref() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：计数器模式缺少渲染配置（内部错误）",
                    );
                    return true;
                };
                let Some(stats) = ctx.counter_stats.as_mut() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：计数器模式缺少统计状态（内部错误）",
                    );
                    return true;
                };
                let Some(renderer) = ctx.counter_renderer.as_mut() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：计数器模式缺少字体渲染器（内部错误）",
                    );
                    return true;
                };
                let out = video_export::render_counter_frame(video_export::CounterFrameInput {
                    frame: &mut frame_data,
                    frame_width: ctx.width,
                    frame_height: ctx.height,
                    document: ctx.document,
                    tick,
                    ppq: ctx.ppq,
                    fps: ctx.fps_f64 as u32,
                    duration_secs: ctx.duration_secs,
                    config: cfg,
                    stats,
                    renderer,
                });
                // CSV 行写入（失败仅告警，不中断渲染）
                if let (Some(line), Some(writer)) = (out.csv_line, ctx.csv_writer.as_mut()) {
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
            RenderMode::NoteRectangle => {
                send_export_error(
                    ctx.progress_tx,
                    "导出失败：NoteRectangle 模式不应进入 CPU 渲染分支（内部错误）",
                );
                return true;
            }
            RenderMode::MIDITrail => {
                send_export_error(
                    ctx.progress_tx,
                    "导出失败：MIDITrail 模式不应进入 CPU 渲染分支（内部错误）",
                );
                return true;
            }
            RenderMode::DataCurve => {
                // 数据曲线模式：统计推进 → 取指标值 → 环形窗口 → 帧渲染
                // 配置缺失 = 内部状态不一致：优雅终止导出，不 panic 渲染线程
                let Some(cfg) = ctx.data_curve_config.as_ref() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：数据曲线模式缺少渲染配置（内部错误）",
                    );
                    return true;
                };
                let Some(stats) = ctx.counter_stats.as_mut() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：数据曲线模式缺少统计状态（内部错误）",
                    );
                    return true;
                };
                let Some(renderer) = ctx.data_curve_renderer.as_mut() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：数据曲线模式缺少渲染器（内部错误）",
                    );
                    return true;
                };
                // 关键：推进统计到当前 tick（与计数器分支一致）。
                // 缺失此调用会导致 NPS/复音数/音符数永远停留在 0 → 曲线为 0 值直线。
                stats.advance(ctx.document, tick, ctx.fps_f64 as u32);
                let value = match cfg.metric {
                    lumino_message::events::window::video::DataCurveMetric::Nps => stats.nps as f64,
                    lumino_message::events::window::video::DataCurveMetric::Polyphony => {
                        stats.polyphony as f64
                    }
                    lumino_message::events::window::video::DataCurveMetric::NoteCount => {
                        stats.note_count as f64
                    }
                    lumino_message::events::window::video::DataCurveMetric::Bpm => {
                        video_export::current_bpm(&ctx.document.tempo_changes, tick)
                    }
                };
                renderer.push_value(value);
                let out = video_export::render_data_curve_frame(
                    &mut frame_data,
                    ctx.width,
                    ctx.height,
                    renderer,
                    cfg,
                );
                // 首帧诊断：确认指标值与缩放状态
                static DATA_CURVE_DIAG: std::sync::atomic::AtomicU32 =
                    std::sync::atomic::AtomicU32::new(0);
                let diag_idx = DATA_CURVE_DIAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if diag_idx < 3 {
                    tracing::info!(
                        "数据曲线模式诊断[{diag_idx}]: tick={tick} value={} zoom=({}, {})",
                        out.value,
                        out.min,
                        out.max,
                    );
                }
            }
            RenderMode::MidiConsole => {
                // MidiConsole 风格：状态化渲染器直出 BGRA；后端由配置决定（GPU/CPU）
                let Some(renderer) = ctx.midi_console_renderer.as_mut() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：MidiConsole 模式缺少渲染器（内部错误）",
                    );
                    return true;
                };
                let Some(cfg) = ctx.midi_console_config.as_ref() else {
                    send_export_error(
                        ctx.progress_tx,
                        "导出失败：MidiConsole 模式缺少渲染配置（内部错误）",
                    );
                    return true;
                };
                match cfg.render_backend {
                    MidiConsoleBackend::Gpu => {
                        video_export::render_midicomsole_frame_gpu(
                            video_export::MidiConsoleFrameArgs {
                                renderer,
                                frame: &mut frame_data,
                                frame_width: ctx.width,
                                frame_height: ctx.height,
                                document: ctx.document,
                                tick,
                                ppq: ctx.ppq,
                                fps: ctx.fps_f64 as u32,
                            },
                        );
                    }
                    MidiConsoleBackend::Cpu => {
                        video_export::render_midicomsole_frame(
                            video_export::MidiConsoleFrameArgs {
                                renderer,
                                frame: &mut frame_data,
                                frame_width: ctx.width,
                                frame_height: ctx.height,
                                document: ctx.document,
                                tick,
                                ppq: ctx.ppq,
                                fps: ctx.fps_f64 as u32,
                            },
                        );
                    }
                }
            }
        }

        // 帧数据直接送入编码通道，跳过渲染线程的 GPU 路径
        if ctx.frame_tx_waterfall.send(frame_data).is_err() {
            tracing::error!("CPU 渲染帧发送失败：通道已关闭");
            send_export_error(ctx.progress_tx, "导出失败：帧通道通信错误");
            return true;
        }
    } else {
        let collect_all = !*ctx.notes_uploaded;
        let Some(params) =
            video_export::build_video_export_render_params(video_export::RenderParamsInput {
                width: ctx.width,
                height: ctx.height,
                tick,
                document: ctx.document,
                ppq: ctx.ppq,
                key_count: ctx.key_count,
                render_mode: ctx.render_mode,
                waterfall_scroll_speed: ctx.waterfall_scroll_speed,
                miditrail_z_far: ctx.miditrail_z_far,
                miditrail_view_mode: ctx.miditrail_view_mode,
                miditrail_normal_speed: ctx.miditrail_normal_speed,
                miditrail_top_speed: ctx.miditrail_top_speed,
                miditrail_3d_notes: ctx.miditrail_3d_notes,
                fps: ctx.fps_f64 as f32,
                visible_notes: ctx.visible_note_buf,
                note_instances_out: ctx.note_instances_buf,
                collect_all,
                window_state: ctx.window_state,
            })
        else {
            send_export_error(
                ctx.progress_tx,
                "导出失败：当前渲染模式不应进入此分支（内部错误）",
            );
            return true;
        };
        // 首帧全量数据已随本帧发出，后续帧只发 uniforms，复用 GPU 常驻数据。
        *ctx.notes_uploaded = true;

        if ctx
            .cmd_sender
            .send(RenderCommand::Control(ControlCommand::RenderVideoFrame {
                params: Box::new(params),
            }))
            .is_err()
        {
            tracing::error!("发送 RenderVideoFrame 命令失败");
            send_export_error(ctx.progress_tx, "导出失败：渲染线程通信错误");
            return true;
        }
    }
    false
}
