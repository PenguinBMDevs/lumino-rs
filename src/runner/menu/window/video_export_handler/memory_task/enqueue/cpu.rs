//! CPU 渲染路径：计数器 / 数据曲线 / MidiConsole 绕过 GPU 直接生成 BGRA 帧。
//!
//! 本模块内的「终止」统一用 `bool` 返回（`true` = 终止渲染循环），由入口
//! `render_cpu_frame` 向上传递到 `enqueue_memory_frame`，不再重复入队帧参数。

use lumino_core::tr;

use super::*;

/// CPU 渲染单帧并送入编码通道；返回 `true` 表示应终止渲染循环。
///
/// 计数器/数据曲线/MidiConsole 模式（CPU 端渲染）：绕过 GPU 开销，BGRA 直出。
/// 注：瀑布流走 GPU compute 管线（见 RenderMode::Waterfall 的 GPU 分支），此处无 CPU 分支。
pub(super) fn render_cpu_frame(ctx: &mut MemoryEnqueueCtx, tick: u32) -> bool {
    let mut frame_data = vec![0u8; (ctx.width as usize) * (ctx.height as usize) * 4];
    let failed = crate::profiling::tracy_zone!("app.cpu_render", {
        match ctx.render_mode {
            RenderMode::Waterfall => {
                send_export_error(
                    ctx.progress_tx,
                    tr!("导出失败：Waterfall 模式不应进入 CPU 渲染分支（内部错误）"),
                );
                true
            }
            RenderMode::NoteCounter => render_note_counter(ctx, tick, &mut frame_data),
            RenderMode::NoteRectangle => {
                send_export_error(
                    ctx.progress_tx,
                    tr!("导出失败：NoteRectangle 模式不应进入 CPU 渲染分支（内部错误）"),
                );
                true
            }
            RenderMode::MIDITrail => {
                send_export_error(
                    ctx.progress_tx,
                    tr!("导出失败：MIDITrail 模式不应进入 CPU 渲染分支（内部错误）"),
                );
                true
            }
            RenderMode::DataCurve => render_data_curve(ctx, tick, &mut frame_data),
            RenderMode::MidiConsole => render_midi_console(ctx, tick, &mut frame_data),
        }
    });
    if failed {
        return true;
    }

    // 帧数据直接送入编码通道，跳过渲染线程的 GPU 路径（CPU 渲染无键色）
    if ctx
        .frame_tx_waterfall
        .send(VideoFrameMessage {
            data: frame_data,
            key_colors: None,
        })
        .is_err()
    {
        tracing::error!("CPU 渲染帧发送失败：通道已关闭");
        send_export_error(ctx.progress_tx, tr!("导出失败：帧通道通信错误"));
        return true;
    }
    false
}

/// 计数器帧：统计推进 + 文本模板渲染；返回 `true` 表示配置缺失需终止。
fn render_note_counter(ctx: &mut MemoryEnqueueCtx, tick: u32, frame_data: &mut [u8]) -> bool {
    // 计数器模式：统计推进 + 文本模板渲染（无卷帘/键盘/标尺）
    // 配置缺失 = 内部状态不一致：优雅终止导出，不 panic 渲染线程
    let Some(cfg) = ctx.counter_config.as_ref() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：计数器模式缺少渲染配置（内部错误）"),
        );
        return true;
    };
    let Some(stats) = ctx.counter_stats.as_mut() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：计数器模式缺少统计状态（内部错误）"),
        );
        return true;
    };
    let Some(renderer) = ctx.counter_renderer.as_mut() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：计数器模式缺少字体渲染器（内部错误）"),
        );
        return true;
    };
    let out = crate::profiling::tracy_zone!("app.counter_frame", {
        video_export::render_counter_frame(video_export::CounterFrameInput {
            frame: frame_data,
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
        })
    });
    // CSV 行写入（失败仅告警，不中断渲染）
    if let (Some(line), Some(writer)) = (out.csv_line, ctx.csv_writer.as_mut()) {
        use std::io::Write;
        if let Err(e) = writeln!(writer, "{line}") {
            tracing::warn!("计数器 CSV 写入失败: {e}");
        }
    }
    // 首帧诊断：确认模板渲染内容（与流式模式诊断风格一致）
    static COUNTER_DIAG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
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
    false
}

/// 数据曲线帧：统计推进 → 取指标值 → 环形窗口 → 帧渲染；返回 `true` 表示终止。
fn render_data_curve(ctx: &mut MemoryEnqueueCtx, tick: u32, frame_data: &mut [u8]) -> bool {
    // 数据曲线模式：统计推进 → 取指标值 → 环形窗口 → 帧渲染
    // 配置缺失 = 内部状态不一致：优雅终止导出，不 panic 渲染线程
    let Some(cfg) = ctx.data_curve_config.as_ref() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：数据曲线模式缺少渲染配置（内部错误）"),
        );
        return true;
    };
    let Some(stats) = ctx.counter_stats.as_mut() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：数据曲线模式缺少统计状态（内部错误）"),
        );
        return true;
    };
    let Some(renderer) = ctx.data_curve_renderer.as_mut() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：数据曲线模式缺少渲染器（内部错误）"),
        );
        return true;
    };
    // 关键：推进统计到当前 tick（与计数器分支一致）。
    // 缺失此调用会导致 NPS/复音数/音符数永远停留在 0 → 曲线为 0 值直线。
    stats.advance(ctx.document, tick, ctx.fps_f64 as u32);
    let value = match cfg.metric {
        lumino_message::events::window::video::DataCurveMetric::Nps => stats.nps as f64,
        lumino_message::events::window::video::DataCurveMetric::Polyphony => stats.polyphony as f64,
        lumino_message::events::window::video::DataCurveMetric::NoteCount => {
            stats.note_count as f64
        }
        lumino_message::events::window::video::DataCurveMetric::Bpm => {
            video_export::current_bpm(&ctx.document.tempo_changes, tick)
        }
    };
    renderer.push_value(value);
    let out = crate::profiling::tracy_zone!("app.data_curve_frame", {
        video_export::render_data_curve_frame(frame_data, ctx.width, ctx.height, renderer, cfg)
    });
    // 首帧诊断：确认指标值与缩放状态
    static DATA_CURVE_DIAG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let diag_idx = DATA_CURVE_DIAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if diag_idx < 3 {
        tracing::info!(
            "数据曲线模式诊断[{diag_idx}]: tick={tick} value={} zoom=({}, {})",
            out.value,
            out.min,
            out.max,
        );
    }
    false
}

/// MidiConsole 帧：状态化渲染器按配置后端直出 BGRA；返回 `true` 表示终止。
fn render_midi_console(ctx: &mut MemoryEnqueueCtx, tick: u32, frame_data: &mut [u8]) -> bool {
    // MidiConsole 风格：状态化渲染器直出 BGRA；后端由配置决定（GPU/CPU）
    let Some(renderer) = ctx.midi_console_renderer.as_mut() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：MidiConsole 模式缺少渲染器（内部错误）"),
        );
        return true;
    };
    let Some(cfg) = ctx.midi_console_config.as_ref() else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：MidiConsole 模式缺少渲染配置（内部错误）"),
        );
        return true;
    };
    crate::profiling::tracy_zone!("app.midi_console_frame", {
        match cfg.render_backend {
            MidiConsoleBackend::Gpu => {
                video_export::render_midicomsole_frame_gpu(video_export::MidiConsoleFrameArgs {
                    renderer,
                    frame: frame_data,
                    frame_width: ctx.width,
                    frame_height: ctx.height,
                    document: ctx.document,
                    tick,
                    ppq: ctx.ppq,
                    fps: ctx.fps_f64 as u32,
                });
            }
            MidiConsoleBackend::Cpu => {
                video_export::render_midicomsole_frame(video_export::MidiConsoleFrameArgs {
                    renderer,
                    frame: frame_data,
                    frame_width: ctx.width,
                    frame_height: ctx.height,
                    document: ctx.document,
                    tick,
                    ppq: ctx.ppq,
                    fps: ctx.fps_f64 as u32,
                });
            }
        }
    });
    false
}
