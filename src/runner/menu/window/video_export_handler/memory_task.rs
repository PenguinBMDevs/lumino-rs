//! 内存模式（完整 MIDI 文档）视频导出后台任务。

use std::io::BufWriter;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Sender, channel};
use std::time::Instant;

use lumino_export::video::{FfmpegEncoder, VideoExportConfig};
use lumino_gfx::render_thread::{ControlCommand, RenderCommand};
use lumino_message::events::window::video::{MidiConsoleBackend, MiditrailViewMode, RenderMode};
use tokio::sync::mpsc::UnboundedSender;

use super::super::video_export::{
    self, CounterFontRenderer, CounterRenderConfig, CounterStats, DataCurveRenderConfig,
    DataCurveRenderer, MidiConsoleRenderConfig, MidiConsoleRenderer, SortableNote, keyboard,
};
use super::commands::{finalize_video_export, send_export_error, send_initial_render_commands};
use super::composite::{CompositeEncodeFrameInput, composite_and_encode_frame};
use super::frame::{EncodeFrameQueue, FrameParams};
use super::pipeline::FramePipeline;

/// 进度消息载荷：(文本, 进度 0..1, 总帧数, 平滑 FPS, 已用秒)
type ProgressMsg = (String, f64, u64, f64, f64);

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
    is_gpu_compute_style: bool,
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
    frame_tx_waterfall: &'a Sender<Vec<u8>>,
    progress_tx: &'a UnboundedSender<ProgressMsg>,
    key_colors: &'a mut [u8; keyboard::KEY_COLOR_BYTES],
    key_color_state: &'a mut keyboard::PlaybackKeyColorState,
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

pub(super) use task::{RunVideoExportTaskInput, run_video_export_task};
