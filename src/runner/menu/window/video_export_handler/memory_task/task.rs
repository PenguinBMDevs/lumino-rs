//! 后台线程主流程：编码器装配 → 配置快照 → 帧循环 → 收尾。
//!
//! 本文件只做阶段编排；各阶段的实现按职责拆分：
//! `encoder_pipeline`（编码器 + 写线程 + 帧通道）、`frame_renderers`（可选渲染器）、
//! `pipeline_run`（帧循环与收尾）、`frame_emit`（单帧合成入队）。

use super::*;

mod encoder_pipeline;
mod frame_emit;
mod frame_renderers;
mod pipeline_run;

use encoder_pipeline::EncoderPipeline;
use frame_renderers::FrameRenderers;
use pipeline_run::FrameLoop;

/// 帧队列缓存的视频时长（秒）：短期编码滞后由此吸收，帧生成速度可独立测量。
const QUEUE_BUFFER_SECONDS: f64 = 2.0;
/// 帧队列内存硬预算（512MiB）：防 300FPS 场景无界堆积 OOM（1080p 单帧 8.29MB）。
const QUEUE_MAX_BYTES: usize = 512 * 1024 * 1024;

/// 输出画面的几何与帧率（导出期间不变）。
pub(super) struct FrameGeometry {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) key_count: u16,
    pub(super) fps_f64: f64,
}

/// 渲染模式及其相关参数（瀑布流 / MIDITrail 视图）。
pub(super) struct RenderStyle {
    pub(super) mode: RenderMode,
    pub(super) is_cpu_renderer: bool,
    pub(super) is_gpu_compute_style: bool,
    pub(super) waterfall_scroll_speed: f32,
    pub(super) miditrail_z_far: f32,
    pub(super) view_mode: MiditrailViewMode,
    pub(super) normal_speed: f32,
    pub(super) top_speed: f32,
    pub(super) has_3d_notes: bool,
}

/// 导出总时长与总帧数。
#[derive(Clone, Copy)]
pub(super) struct FrameTotals {
    pub(super) duration_secs: f64,
    pub(super) total_frames: u64,
}

impl FrameTotals {
    /// 由文档速度表与配置算出总时长与总帧数。
    fn compute(
        document: &lumino_midi_loader::MidiDocument,
        config: &VideoExportConfig,
        ppq: u32,
    ) -> Self {
        let duration_secs =
            video_export::compute_duration_secs(&document.tempo_changes, document.total_ticks, ppq);
        Self {
            duration_secs,
            total_frames: config.total_frames(duration_secs),
        }
    }
}

/// 装配阶段产物：原始输入（帧循环还要读配置）+ 由此派生的快照。
struct PreparedExport {
    input: RunVideoExportTaskInput,
    start: Instant,
    geometry: FrameGeometry,
    totals: FrameTotals,
    renderers: FrameRenderers,
    pipe: EncoderPipeline,
}

/// 后台线程主流程：装配编码器后进入帧循环，结束时带上 TDR 标记收尾。
///
/// 该函数整体等价于原 `handle_start_video_export` 中 `move` 闭包体内的逻辑，
/// 仅按阶段拆到子模块，行为保持一致。
pub(crate) fn run_video_export_task(input: RunVideoExportTaskInput) {
    let Some(prepared) = prepare_export(input) else {
        return;
    };
    run_prepared_export(prepared);
}

/// 装配阶段：编码器 + 写线程 + 初始渲染命令 + 配置快照 + 渲染器。
///
/// 任一步失败都已在 `progress_tx` 上报，返回 `None` 表示任务直接结束。
fn prepare_export(input: RunVideoExportTaskInput) -> Option<PreparedExport> {
    let start = Instant::now();
    let geometry = FrameGeometry {
        width: input.width,
        height: input.height,
        key_count: input.key_count,
        fps_f64: input.fps_f64,
    };

    // 编码器 + ffmpeg 写线程 + 帧通道
    let (pipe, frame_tx, recycle_rx) = EncoderPipeline::spawn(
        &input.config,
        input.input_pix_fmt,
        &geometry,
        &input.cancel_flag,
        &input.progress_tx,
    )?;

    // 渲染线程的 `StartVideoExport` 携带帧发送端与缓冲回收端；通信失败即终止
    if send_initial_render_commands(
        &input.cmd_sender,
        input.width,
        input.height,
        frame_tx,
        recycle_rx,
        &input.progress_tx,
    ) {
        return None;
    }

    Some(PreparedExport {
        totals: FrameTotals::compute(&input.document, &input.config, input.ppq),
        renderers: FrameRenderers::new(
            &input.document,
            input.counter_config.as_ref(),
            input.data_curve_config.as_ref(),
            input.midi_console_config.as_ref(),
            input.fps_f64,
        ),
        input,
        geometry,
        start,
        pipe,
    })
}

/// 帧循环阶段：建立进度条与 `FrameLoop`，跑完主循环后收尾。
fn run_prepared_export(prepared: PreparedExport) {
    let PreparedExport {
        input,
        geometry,
        totals,
        renderers,
        pipe,
        start,
    } = prepared;

    let mut render_bar = video_export::cli_progress::CliProgressBar::new(30, "视频渲染");
    render_bar.update(
        0.0,
        &format!(
            "总时长 {:.1}s | 总帧数 {} | PPQ {}",
            totals.duration_secs, totals.total_frames, input.ppq
        ),
    );

    let mut frame_loop = FrameLoop {
        cmd_sender: &input.cmd_sender,
        progress_tx: &input.progress_tx,
        preview_tx: &input.preview_tx,
        document: &input.document,
        cancel_flag: &input.cancel_flag,
        geometry,
        style: RenderStyle {
            mode: input.render_mode,
            is_cpu_renderer: input.is_cpu_renderer,
            is_gpu_compute_style: input.is_gpu_compute_style,
            waterfall_scroll_speed: input.waterfall_scroll_speed,
            miditrail_z_far: input.miditrail_z_far,
            view_mode: input.miditrail_view_mode,
            normal_speed: input.miditrail_normal_speed,
            top_speed: input.miditrail_top_speed,
            has_3d_notes: input.miditrail_3d_notes,
        },
        ppq: input.ppq,
        totals,
        counter_config: input.counter_config,
        data_curve_config: input.data_curve_config,
        midi_console_config: input.midi_console_config,
        pipe,
        renderers,
        start,
    };
    let outcome = frame_loop.run(&mut render_bar);
    frame_loop.finish(
        outcome,
        &mut render_bar,
        input.tdr_cancel_flag.load(Ordering::SeqCst),
    );
}
