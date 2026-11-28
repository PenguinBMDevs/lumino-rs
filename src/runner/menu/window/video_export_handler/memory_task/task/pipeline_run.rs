//! 内存导出帧循环与收尾。
//!
//! `FrameLoop` 持有一次导出的全部运行期状态（配置快照、编码器、渲染器）；
//! `run` 驱动公共 `FramePipeline`（预填充 → 主循环 → drain），
//! `finish` 只做收尾：刷新 CSV、写终态进度条、交 `finalize_video_export`。
//! 状态与行为放在同一文件，避免「同一 struct 的 impl 散落多文件」的伪拆分。

use super::enqueue::enqueue_memory_frame;
use super::frame_emit::{FrameEmitCtx, emit_frame};
use super::*;

/// 帧生成复用的缓冲区与滑动状态（避免每帧堆分配）。
struct FrameBuffers {
    visible_note_buf: Vec<SortableNote>,
    note_instances_buf: Vec<lumino_gfx::NoteInstance>,
    /// 首帧全量上传标记：首帧收集全文档音符常驻 GPU，后续帧跳过收集（uniform 驱动重裁剪）。
    notes_uploaded: bool,
    /// 瀑布流/MIDITrail 窗口收集滑动状态（同一导出任务内复用，tick 单调递增）。
    window_state: video_export::WindowCollectState,
}

impl Default for FrameBuffers {
    fn default() -> Self {
        Self {
            visible_note_buf: Vec::with_capacity(4096),
            note_instances_buf: Vec::with_capacity(4096),
            notes_uploaded: false,
            window_state: video_export::WindowCollectState::default(),
        }
    }
}

/// 主循环结果：已处理帧数、是否取消、平滑帧率。
pub(super) struct LoopOutcome {
    pub(super) processed_frames: u64,
    pub(super) cancelled: bool,
    pub(super) smoothed_fps: f64,
}

/// 一次内存导出的运行期状态。
///
/// 字段是原 `run_video_export_task` 局部变量的平铺集合：配置快照（几何/风格/时长）、
/// 三份可选配置、编码流水线与渲染器。字段虽多但彼此独立，收尾时整体消耗。
pub(super) struct FrameLoop<'a> {
    pub(super) cmd_sender: &'a Sender<RenderCommand>,
    pub(super) progress_tx: &'a UnboundedSender<ProgressMsg>,
    pub(super) preview_tx: &'a UnboundedSender<(Vec<u8>, u32, u32)>,
    pub(super) document: &'a Arc<lumino_midi_loader::MidiDocument>,
    pub(super) cancel_flag: &'a Arc<AtomicBool>,
    pub(super) geometry: FrameGeometry,
    pub(super) style: RenderStyle,
    pub(super) ppq: u32,
    pub(super) totals: FrameTotals,
    pub(super) counter_config: Option<CounterRenderConfig>,
    pub(super) data_curve_config: Option<DataCurveRenderConfig>,
    pub(super) midi_console_config: Option<MidiConsoleRenderConfig>,
    pub(super) pipe: EncoderPipeline,
    pub(super) renderers: FrameRenderers,
    pub(super) start: Instant,
}

/// 诊断帧上限（Tracy 基线抓固定窗口；正常导出不设该环境变量）。
fn bench_frame_limit() -> Option<u64> {
    let limit = std::env::var("LUMINO_BENCH_FRAMES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0);
    if let Some(limit) = limit {
        tracing::info!("诊断帧上限生效：{limit} 帧（LUMINO_BENCH_FRAMES）");
    }
    limit
}

impl FrameLoop<'_> {
    /// 帧流水线主循环：补发渲染命令（`enqueue_frame`）+ 收帧合成编码（`emit_frame`）。
    pub(super) fn run(
        &mut self,
        bar: &mut video_export::cli_progress::CliProgressBar,
    ) -> LoopOutcome {
        let mut buffers = FrameBuffers::default();
        let mut last_preview_time = Instant::now();
        let mut preview_sent = false;

        // ★ 生成键盘贴图（使用 CPU 贴图方式，在帧数据上合成）
        let (keyboard_pixels, kb_w, kb_h) = video_export::generate_keyboard_texture(
            self.geometry.width,
            self.geometry.height,
            self.geometry.key_count,
        );

        // 流水线渲染：Runner 预填充 4 帧命令，让 staging ring 从开始就满载，
        // 之后每处理完一帧立即补发下一帧，保持 GPU/CPU 流水线持续运转。
        // 每帧参数携带该帧的按键高亮颜色（RGBAx256 键），用于后台线程合成键盘。
        let mut param_queue = EncodeFrameQueue::with_capacity(16);

        // 闭包不捕获 param_queue，而是作为参数传入，避免与主循环中的 pop_front 产生可变借用冲突。
        let mut ctx = MemoryEnqueueCtx {
            cmd_sender: self.cmd_sender,
            document: self.document,
            fps_f64: self.geometry.fps_f64,
            ppq: self.ppq,
            width: self.geometry.width,
            height: self.geometry.height,
            key_count: self.geometry.key_count,
            is_cpu_renderer: self.style.is_cpu_renderer,
            render_mode: self.style.mode,
            counter_config: &self.counter_config,
            counter_stats: &mut self.renderers.counter_stats,
            counter_renderer: &mut self.renderers.counter_renderer,
            data_curve_config: &self.data_curve_config,
            data_curve_renderer: &mut self.renderers.data_curve_renderer,
            midi_console_config: &self.midi_console_config,
            midi_console_renderer: &mut self.renderers.midi_console_renderer,
            duration_secs: self.totals.duration_secs,
            waterfall_scroll_speed: self.style.waterfall_scroll_speed,
            miditrail_z_far: self.style.miditrail_z_far,
            miditrail_view_mode: self.style.view_mode,
            miditrail_normal_speed: self.style.normal_speed,
            miditrail_top_speed: self.style.top_speed,
            miditrail_3d_notes: self.style.has_3d_notes,
            frame_tx_waterfall: &self.pipe.frame_tx_waterfall,
            progress_tx: self.progress_tx,
            csv_writer: &mut self.renderers.csv_writer,
            visible_note_buf: &mut buffers.visible_note_buf,
            note_instances_buf: &mut buffers.note_instances_buf,
            notes_uploaded: &mut buffers.notes_uploaded,
            window_state: &mut buffers.window_state,
        };
        let mut emit = FrameEmitCtx {
            width: self.geometry.width,
            height: self.geometry.height,
            skip_chrome: self.style.is_gpu_compute_style || self.style.is_cpu_renderer,
            keyboard_pixels: &keyboard_pixels,
            keyboard_w: kb_w,
            keyboard_h: kb_h,
            preview_tx: self.preview_tx,
            last_preview_time: &mut last_preview_time,
            preview_sent: &mut preview_sent,
            encoder_thread: &mut self.pipe.thread,
            cancel_flag: self.cancel_flag,
            progress_tx: self.progress_tx,
        };
        let mut enqueue_frame = |queue: &mut EncodeFrameQueue, frame_idx: u64| -> bool {
            enqueue_memory_frame(&mut ctx, queue, frame_idx)
        };
        let mut process_frame =
            |message: VideoFrameMessage, frame_params: FrameParams| -> (bool, FrameStageStats) {
                emit_frame(&mut emit, message, frame_params)
            };

        let frame_limit = bench_frame_limit();

        // 预填充 + 主循环 + drain 由公共 FramePipeline 驱动（与流式路径共用同一循环骨架）
        let mut pipeline = FramePipeline {
            total_frames: self.totals.total_frames,
            frame_limit,
            cancel_flag: self.cancel_flag,
            frame_rx: &self.pipe.frame_rx,
            param_queue: &mut param_queue,
            progress_tx: self.progress_tx,
            render_bar: bar,
            start: self.start,
            // 内存路径：进度直接映射（无解析阶段）
            progress_map: |p| p,
            telemetry: &self.pipe.telemetry,
        };
        let (processed_frames, cancelled, smoothed_fps) =
            pipeline.run(&mut enqueue_frame, &mut process_frame);
        LoopOutcome {
            processed_frames,
            cancelled,
            smoothed_fps,
        }
    }

    /// 收尾：刷新 CSV、写终态进度条、交 `finalize_video_export`（必发 FinishVideoExport）。
    pub(super) fn finish(
        mut self,
        outcome: LoopOutcome,
        bar: &mut video_export::cli_progress::CliProgressBar,
        tdr_cancelled: bool,
    ) {
        let elapsed = self.start.elapsed().as_secs_f64();
        let encode_failed = self.pipe.telemetry.failed.load(Ordering::Relaxed);
        if let Some(writer) = self.renderers.csv_writer.as_mut() {
            use std::io::Write;
            if let Err(e) = writer.flush() {
                tracing::warn!("计数器 CSV 收尾失败: {e}");
            }
        }
        // 完成编码：无论是否取消都必须调用 finish()，
        // 否则 FFmpeg 收不到 EOF，视频文件头未写入导致损坏。
        // 用户取消时已写入的帧仍可生成可播放的部分视频。
        if encode_failed {
            bar.finish(&format!(
                "编码失败 | 已入队 {}/{} 帧 | 耗时 {:.1}s",
                outcome.processed_frames, self.totals.total_frames, elapsed
            ));
        } else if outcome.cancelled {
            bar.finish(&format!(
                "已取消 | 已入队 {}/{} 帧 | 耗时 {:.1}s",
                outcome.processed_frames, self.totals.total_frames, elapsed
            ));
        } else {
            bar.finish(&format!(
                "完成 {}/{} 帧 | 耗时 {:.1}s",
                outcome.processed_frames, self.totals.total_frames, elapsed
            ));
        }
        finalize_video_export(
            self.cmd_sender,
            self.pipe.thread,
            outcome.cancelled,
            tdr_cancelled,
            encode_failed,
            elapsed,
            self.totals.total_frames,
            outcome.smoothed_fps,
            self.progress_tx,
        );
    }
}
