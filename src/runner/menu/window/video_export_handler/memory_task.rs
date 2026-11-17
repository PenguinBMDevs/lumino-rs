//! 内存模式（完整 MIDI 文档）视频导出后台任务。

use std::io::BufWriter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::Instant;

use lumino_export::video::{FfmpegEncoder, VideoExportConfig};
use lumino_gfx::render_thread::{ControlCommand, RenderCommand, VideoFrameMessage};
use lumino_message::events::window::video::{MidiConsoleBackend, MiditrailViewMode, RenderMode};
use tokio::sync::mpsc::UnboundedSender;

use super::super::video_export::{
    self, CounterFontRenderer, CounterRenderConfig, CounterStats, DataCurveRenderConfig,
    DataCurveRenderer, MidiConsoleRenderConfig, MidiConsoleRenderer, SortableNote,
};
use super::commands::{finalize_video_export, send_export_error, send_initial_render_commands};
use super::composite::{CompositeFrameInput, composite_frame};
use super::encoder_thread::{EncoderThread, EnqueueOutcome};
use super::frame::{
    EncodeFrameQueue, EncodeTelemetry, FrameParams, FrameQueueConfig, FrameStageStats,
    PIPELINE_DEPTH, ProgressMsg,
};
use super::pipeline::FramePipeline;

/// 后台视频导出任务输入（由窗口事件处理层组装后交后台线程消费）。
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
    /// TDR 归因标志（R5-b）：重连取消时置位——收尾按失败上报而非「已取消」
    pub tdr_cancel_flag: Arc<AtomicBool>,
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

/// 内存模式入队阶段共享状态（原 `run_video_export_task` 内嵌闭包捕获的全部变量）。
struct MemoryEnqueueCtx<'a> {
    cmd_sender: &'a Sender<RenderCommand>,
    document: &'a Arc<lumino_midi_loader::MidiDocument>,
    fps_f64: f64,
    ppq: u32,
    width: u32,
    height: u32,
    key_count: u16,
    is_cpu_renderer: bool,
    render_mode: RenderMode,
    counter_config: &'a Option<CounterRenderConfig>,
    counter_stats: &'a mut Option<CounterStats>,
    counter_renderer: &'a mut Option<CounterFontRenderer>,
    data_curve_config: &'a Option<DataCurveRenderConfig>,
    data_curve_renderer: &'a mut Option<DataCurveRenderer>,
    midi_console_config: &'a Option<MidiConsoleRenderConfig>,
    midi_console_renderer: &'a mut Option<MidiConsoleRenderer>,
    duration_secs: f64,
    waterfall_scroll_speed: f32,
    miditrail_z_far: f32,
    miditrail_view_mode: MiditrailViewMode,
    miditrail_normal_speed: f32,
    miditrail_top_speed: f32,
    miditrail_3d_notes: bool,
    frame_tx_waterfall: &'a Sender<VideoFrameMessage>,
    progress_tx: &'a UnboundedSender<ProgressMsg>,
    csv_writer: &'a mut Option<BufWriter<std::fs::File>>,
    visible_note_buf: &'a mut Vec<SortableNote>,
    note_instances_buf: &'a mut Vec<lumino_gfx::NoteInstance>,
    /// 首帧全量上传标记：首帧收集全文档音符常驻 GPU，后续帧跳过收集（uniform 驱动重裁剪）。
    notes_uploaded: &'a mut bool,
    /// 瀑布流/MIDITrail 窗口收集滑动状态（同一导出任务内复用，tick 单调递增）。
    window_state: &'a mut video_export::WindowCollectState,
}

mod enqueue;
mod task;

pub(super) use task::run_video_export_task;
