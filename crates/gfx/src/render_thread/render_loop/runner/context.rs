use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use lumino_onion_skin::OnionSkinNote;

use crate::render_thread::commands::{FrameSender, RenderCommand};
use crate::render_thread::export_pipeline::ExportPipeline;
use crate::render_thread::params::RenderParams;
use crate::render_thread::render_loop::Renderers;
use crate::render_thread::render_loop::runner::types::{HiResMeta, HiResStreamMsg};
use crate::render_thread::stats::RenderStats;
use crate::{
    GroupTile, HiResConfig, HiResRenderer, MiditrailRenderer, SwappableBuffer, WaterfallRenderer,
};

/// 不可变 GPU 基础设施。
///
/// 聚合渲染线程中反复传递的 device / queue / texture_format 三元组，
/// 消除 8 个函数中重复的 wgpu 参数传递。
#[derive(Debug)]
pub struct RenderContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub texture_format: wgpu::TextureFormat,
}

impl RenderContext {
    /// 使用已有的 device / queue / texture_format 构造。
    #[must_use]
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            device,
            queue,
            texture_format,
        }
    }
}

/// 渲染线程间通信通道与共享状态。
///
/// 聚合 `run_render_thread` 入口处从外部传入的所有跨线程共享资源，
/// 将 8 个通道 / 原子 / 互斥参数压缩为 1 个结构体。
pub struct RenderThreadChannels {
    /// 线程运行标志
    pub running: Arc<AtomicBool>,
    /// 命令接收端（UI 线程 → 渲染线程）
    pub command_receiver: std::sync::mpsc::Receiver<RenderCommand>,
    /// 离屏纹理共享引用（渲染线程写入最新纹理 → 主线程复制到 Surface）
    pub latest_texture_clone: Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    /// 渲染统计共享引用（渲染线程写入 → UI 线程读取）
    pub stats_clone: Arc<Mutex<RenderStats>>,
    /// 音符事件接收端（UI 线程 → 渲染线程增量更新）
    pub note_events_rx: std::sync::mpsc::Receiver<crate::NoteEvent>,
    /// 双缓冲音符实例数据（UI 线程写入，渲染线程读取）
    pub note_instances_buffer: Arc<SwappableBuffer<crate::NoteInstance>>,
    /// 洋葱皮生成进度缓冲（渲染线程写入，UI 线程读取并转发到进度窗口）
    pub onion_progress: Arc<Mutex<Vec<(String, f32)>>>,
    /// 洋葱皮流式上传接收端（UI 线程分块构建 → 渲染线程 streaming_append）
    ///
    /// 空 Vec 表示流式上传完成。
    pub onion_skin_streaming_rx: std::sync::mpsc::Receiver<Vec<crate::NoteInstance>>,
}

/// 可变的每帧渲染状态。
///
/// 聚合渲染循环中所有可变帧局部状态，使 `handle_video_frame`、
/// `execute_render_pass` 等函数的签名从 14–19 个参数降至 4–6 个。
///
/// 所有 `&mut` 字段在只读场景下可通过 `as_ref()` 或 `&*` 自动重借入为 `&`。
pub struct RenderFrameState<'a> {
    /// 5 个渲染器（网格、音符、标尺、走带、CC 柱状条）
    pub renderers: &'a mut Renderers,
    /// 当前帧的离屏渲染纹理
    pub current_texture: &'a mut Option<Arc<wgpu::Texture>>,
    /// 当前帧的深度纹理
    pub depth_texture: &'a mut Option<wgpu::Texture>,
    /// 当前帧的深度纹理视图
    pub depth_texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前帧的离屏渲染纹理视图（缓存，避免每帧 create_view）
    pub texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前视口尺寸
    pub current_size: &'a mut (u32, u32),
    /// 音符实例版本号（检测是否需重上传）
    pub last_note_version: &'a mut u64,
    /// 离屏纹理共享引用（ensure_textures 中设置为最新纹理给主线程）
    pub latest_texture_clone: &'a Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    /// 高精度洋葱皮渲染器
    pub hires_renderer: &'a mut Option<HiResRenderer>,
    /// 高精度贴图元数据（视口计算用）
    pub hires_meta: &'a mut Option<HiResMeta>,
    /// 高精度贴图配置
    pub hires_config: &'a mut Option<HiResConfig>,
    /// 视频导出读回管线
    pub export_pipeline: &'a mut Option<ExportPipeline>,
    /// 视频帧数据发送器
    pub export_frame_tx: &'a mut Option<FrameSender>,
    /// 瀑布流 GPU 渲染器（视频导出使用）
    pub waterfall_renderer: &'a mut Option<crate::WaterfallRenderer>,
    /// Miditrail 3D GPU 渲染器（视频导出使用）
    pub miditrail_renderer: &'a mut Option<crate::MiditrailRenderer>,
}

/// 视频导出高精度贴图上传参数。
///
/// 将 `upload_hires_video_tiles` 中 6 个命令来源的参数打包为一个结构体，
/// 使函数签名降至 3 个参数（ctx + frame + params）。
pub struct UploadHiResTileParams {
    /// 整合组贴图列表
    pub tiles: Vec<GroupTile>,
    /// 高精度贴图配置
    pub config: HiResConfig,
    /// 音轨总数
    pub track_count: u16,
    /// 键位数量（128 或 256）
    pub key_count: u16,
    /// 全曲总 tick
    pub total_ticks: u32,
    /// MIDI ppq
    pub ppq: u16,
}

/// 预览帧纹理上传阶段上下文。
///
/// 聚合 `ensure_offscreen_textures_and_upload_notes` 中分散的 device、通道、
/// 渲染器、纹理状态与音符版本参数，避免函数签名超过 clippy 阈值。
pub(crate) struct PreviewUploadContext<'a> {
    /// 不可变 GPU 基础设施
    pub(crate) ctx: &'a RenderContext,
    /// 跨线程通信通道
    pub(crate) channels: &'a RenderThreadChannels,
    /// 主渲染器集合
    pub(crate) renderers: &'a mut Renderers,
    /// 当前离屏纹理
    pub(crate) current_texture: &'a mut Option<Arc<wgpu::Texture>>,
    /// 当前深度纹理
    pub(crate) depth_texture: &'a mut Option<wgpu::Texture>,
    /// 当前深度纹理视图
    pub(crate) depth_texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理视图
    pub(crate) texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理尺寸
    pub(crate) current_size: &'a mut (u32, u32),
    /// 上次上传的音符版本
    pub(crate) last_note_version: &'a mut u64,
    /// 渲染参数
    pub(crate) params: &'a RenderParams,
}

/// 预览渲染通道上下文。
///
/// 聚合 `render_offscreen_pass` 中需要的全部帧状态与渲染资源。
pub(crate) struct PreviewPassContext<'a> {
    /// 不可变 GPU 基础设施
    pub(crate) ctx: &'a RenderContext,
    /// 当前帧渲染参数
    pub(crate) params: &'a RenderParams,
    /// 跨线程通信通道
    pub(crate) channels: &'a RenderThreadChannels,
    /// 主渲染器集合
    pub(crate) renderers: &'a mut Renderers,
    /// 当前离屏纹理
    pub(crate) current_texture: &'a mut Option<Arc<wgpu::Texture>>,
    /// 当前深度纹理
    pub(crate) depth_texture: &'a mut Option<wgpu::Texture>,
    /// 当前深度纹理视图
    pub(crate) depth_texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理视图
    pub(crate) texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理尺寸
    pub(crate) current_size: &'a mut (u32, u32),
    /// 上次上传的音符版本
    pub(crate) last_note_version: &'a mut u64,
    /// 高精度洋葱皮渲染器
    pub(crate) hires_renderer: &'a mut Option<HiResRenderer>,
    /// 高精度贴图元数据
    pub(crate) hires_meta: &'a mut Option<HiResMeta>,
    /// 高精度贴图配置
    pub(crate) hires_config: &'a mut Option<HiResConfig>,
    /// 视频导出读回管线
    pub(crate) export_pipeline: &'a mut Option<ExportPipeline>,
    /// 视频帧发送器
    pub(crate) export_frame_tx: &'a mut Option<FrameSender>,
}

/// 视频导出启动阶段上下文。
///
/// 聚合 `start_video_export` 中初始化导出管线所需参数。
pub(crate) struct VideoExportSetupContext<'a> {
    /// 不可变 GPU 基础设施
    pub(crate) ctx: &'a RenderContext,
    /// 导出帧宽度
    pub(crate) width: u32,
    /// 导出帧高度
    pub(crate) height: u32,
    /// 视频帧发送通道
    pub(crate) frame_tx: FrameSender,
    /// 像素缓冲回收通道
    pub(crate) recycle_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    /// 导出管线
    pub(crate) export_pipeline: &'a mut Option<ExportPipeline>,
    /// 视频帧发送器
    pub(crate) export_frame_tx: &'a mut Option<FrameSender>,
    /// 视频导出专用渲染器
    pub(crate) export_renderers: &'a mut Option<Renderers>,
}

/// 视频导出帧渲染上下文。
///
/// 聚合 `render_video_frame_command` 中每帧渲染所需参数。
pub(crate) struct VideoExportFrameContext<'a> {
    /// 不可变 GPU 基础设施
    pub(crate) ctx: &'a RenderContext,
    /// 跨线程通信通道
    pub(crate) channels: &'a RenderThreadChannels,
    /// 当前帧渲染参数
    pub(crate) params: RenderParams,
    /// 视频导出专用渲染器
    pub(crate) export_renderers: &'a mut Option<Renderers>,
    /// 主渲染器集合
    pub(crate) renderers: &'a mut Renderers,
    /// 当前离屏纹理
    pub(crate) current_texture: &'a mut Option<Arc<wgpu::Texture>>,
    /// 当前深度纹理
    pub(crate) depth_texture: &'a mut Option<wgpu::Texture>,
    /// 当前深度纹理视图
    pub(crate) depth_texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理视图
    pub(crate) texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理尺寸
    pub(crate) current_size: &'a mut (u32, u32),
    /// 上次上传的音符版本
    pub(crate) last_note_version: &'a mut u64,
    /// 高精度洋葱皮渲染器
    pub(crate) hires_renderer: &'a mut Option<HiResRenderer>,
    /// 高精度贴图元数据
    pub(crate) hires_meta: &'a mut Option<HiResMeta>,
    /// 高精度贴图配置
    pub(crate) hires_config: &'a mut Option<HiResConfig>,
    /// 视频导出读回管线
    pub(crate) export_pipeline: &'a mut Option<ExportPipeline>,
    /// 视频帧发送器
    pub(crate) export_frame_tx: &'a mut Option<FrameSender>,
    /// 瀑布流渲染器
    pub(crate) waterfall_renderer: &'a mut Option<WaterfallRenderer>,
    /// Miditrail 渲染器
    pub(crate) miditrail_renderer: &'a mut Option<MiditrailRenderer>,
}

/// 高精度视频贴图上传上下文。
///
/// 聚合 `upload_hires_video_tiles_command` 中需要的高精度渲染状态。
pub(crate) struct HiResUploadContext<'a> {
    /// 不可变 GPU 基础设施
    pub(crate) ctx: &'a RenderContext,
    /// 高精度洋葱皮渲染器
    pub(crate) hires_renderer: &'a mut Option<HiResRenderer>,
    /// 高精度贴图元数据
    pub(crate) hires_meta: &'a mut Option<HiResMeta>,
    /// 高精度贴图配置
    pub(crate) hires_config: &'a mut Option<HiResConfig>,
}

/// 高精度后台生成上下文。
///
/// 聚合 `handle_generate_hires` 中后台线程生成高精度贴图所需的全部参数。
/// owned 字段在生成前被移入后台线程；引用字段在生成前完成初始化。
pub(crate) struct HiResGenerateContext<'a> {
    /// 不可变 GPU 基础设施
    pub(crate) ctx: &'a RenderContext,
    /// 每轨音符列表
    pub(crate) notes: Vec<Vec<OnionSkinNote>>,
    /// MIDI ppq
    pub(crate) ppq: u16,
    /// 键位数量
    pub(crate) key_count: u16,
    /// 全曲总 tick
    pub(crate) total_ticks: u32,
    /// 高精度贴图配置
    pub(crate) config: HiResConfig,
    /// MIDI 内容哈希
    pub(crate) midi_hash: String,
    /// 高精度结果发送通道
    pub(crate) hires_result_tx: &'a std::sync::mpsc::SyncSender<HiResStreamMsg>,
    /// 洋葱皮进度缓冲
    pub(crate) onion_progress: &'a Arc<Mutex<Vec<(String, f32)>>>,
    /// 高精度洋葱皮渲染器
    pub(crate) hires_renderer: &'a mut Option<HiResRenderer>,
    /// 高精度贴图元数据
    pub(crate) hires_meta: &'a mut Option<HiResMeta>,
    /// 高精度贴图配置
    pub(crate) hires_config: &'a mut Option<HiResConfig>,
}

/// 延迟控制命令处理上下文。
///
/// 聚合 `handle_deferred_command` 与 `process_deferred_commands` 中需要的状态。
/// 使用独立字段而非嵌套 `RenderFrameState`，避免与 `channels` 发生借用冲突。
pub(crate) struct DeferredCommandContext<'a> {
    /// 不可变 GPU 基础设施
    pub(crate) ctx: &'a RenderContext,
    /// 跨线程通信通道
    pub(crate) channels: &'a RenderThreadChannels,
    /// 主渲染器集合
    pub(crate) renderers: &'a mut Renderers,
    /// 视频导出专用渲染器
    pub(crate) export_renderers: &'a mut Option<Renderers>,
    /// 当前离屏纹理
    pub(crate) current_texture: &'a mut Option<Arc<wgpu::Texture>>,
    /// 当前深度纹理
    pub(crate) depth_texture: &'a mut Option<wgpu::Texture>,
    /// 当前深度纹理视图
    pub(crate) depth_texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理视图
    pub(crate) texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前纹理尺寸
    pub(crate) current_size: &'a mut (u32, u32),
    /// 上次上传的音符版本
    pub(crate) last_note_version: &'a mut u64,
    /// 高精度洋葱皮渲染器
    pub(crate) hires_renderer: &'a mut Option<HiResRenderer>,
    /// 高精度贴图元数据
    pub(crate) hires_meta: &'a mut Option<HiResMeta>,
    /// 高精度贴图配置
    pub(crate) hires_config: &'a mut Option<HiResConfig>,
    /// 视频导出读回管线
    pub(crate) export_pipeline: &'a mut Option<ExportPipeline>,
    /// 视频帧发送器
    pub(crate) export_frame_tx: &'a mut Option<FrameSender>,
    /// 瀑布流渲染器
    pub(crate) waterfall_renderer: &'a mut Option<WaterfallRenderer>,
    /// Miditrail 渲染器
    pub(crate) miditrail_renderer: &'a mut Option<MiditrailRenderer>,
    /// 高精度结果发送通道
    pub(crate) hires_result_tx: &'a std::sync::mpsc::SyncSender<HiResStreamMsg>,
}
