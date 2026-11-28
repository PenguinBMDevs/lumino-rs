use std::sync::{Arc, atomic::Ordering};
use std::time::Instant;

use super::super::super::commands::ControlCommand;
use super::super::super::export_pipeline::ExportPipeline;
use super::super::super::params::RenderParams;
use super::super::commands::process_commands;
use super::super::render_pass::update_stats;
use super::context::{
    DeferredCommandContext, PreviewPassContext, PreviewUploadContext, RenderContext,
    RenderThreadChannels,
};
use super::onion_segments::{OnionSegment, process_main_track_events};
use super::preview::{ensure_offscreen_textures_and_upload_notes, render_offscreen_pass};
use super::video_export::advance_export_inflight;
use crate::gpu_resource_tracker::TrackedTexture;
use lumino_midiplayer::texture_waterfall::{
    WaterfallGpuCtx, WaterfallStreamMsg, drain_waterfall_stream,
};

mod arrangement;
mod deferred_commands;
mod onion_stream;

use arrangement::prepare_arrangement_note_data;
use deferred_commands::process_deferred_commands;
use onion_stream::drain_onion_skin_stream;

/// 运行渲染线程主循环（panic 兜底外层）。
///
/// REND-019（PR-A §1.6）：渲染循环 panic 不得裸逃逸——外层 `catch_unwind`
/// 捕获后置位停帧标志并统一收尾（错误响应兜底）。注意 release
/// （`panic = "abort"`）下进程终止由 profile 兜底，catch_unwind 覆盖
/// unwind 构建（debug / CI）。
pub fn run_render_thread(ctx: RenderContext, channels: RenderThreadChannels) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_render_thread_inner(ctx, channels);
    }));
    if let Err(payload) = result {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "未知 panic 载荷".to_string());
        tracing::error!("渲染线程 panic 被兜底捕获：{detail}；置位停帧标志（错误响应兜底）");
        let _ = crate::device_status::mark_device_lost();
    }
    tracing::info!("Render thread stopped");
}

/// 离屏纹理与视图状态（渲染线程私有，跨帧复用）。
struct LoopTextures {
    current: Option<Arc<TrackedTexture>>,
    depth: Option<TrackedTexture>,
    depth_view: Option<wgpu::TextureView>,
    view: Option<wgpu::TextureView>,
}

/// 贴图瀑布流 / 视频导出专用渲染器状态（跨帧复用，避免每帧重建 pipeline）。
struct LoopWaterfall {
    renderer: Option<crate::TextureWaterfallRenderer>,
    meta: Option<crate::WaterfallMeta>,
    config: Option<crate::TextureWaterfallConfig>,
    video_renderer: Option<crate::WaterfallRenderer>,
    miditrail_renderer: Option<crate::MiditrailRenderer>,
}

/// 视频导出读回管线状态。
struct LoopExport {
    pipeline: Option<ExportPipeline>,
    frame_tx: Option<crate::render_thread::commands::FrameSender>,
}

/// 贴图瀑布流流式上传 / 主音轨事件状态。
struct LoopOnion {
    streaming_in_progress: bool,
    segments: Vec<OnionSegment>,
    current_track_encoded: u32,
}

/// 渲染循环持有的全部可变状态（按职责分组，满足 DEBT-10 字段数上限）。
struct LoopState {
    renderers: super::super::Renderers,
    export_renderers: Option<super::super::Renderers>,
    frame_count: u64,
    fps_update_time: Instant,
    latest_frame_id: u64,
    textures: LoopTextures,
    current_size: (u32, u32),
    last_note_version: u64,
    waterfall: LoopWaterfall,
    deferred: Vec<ControlCommand>,
    export: LoopExport,
    onion: LoopOnion,
}

impl LoopState {
    fn new(ctx: &RenderContext) -> Self {
        // 初始化渲染器
        let renderers =
            super::super::Renderers::new(&ctx.device, &ctx.queue, ctx.texture_format, ctx.epoch);
        // 视频导出使用独立的纯 2D 渲染器，避免 depth-stencil 状态与普通预览不一致。
        let export_renderers = None;

        // 渲染循环状态
        let frame_count = 0u64;
        let fps_update_time = Instant::now();
        // 记录本轮回环处理到的最后一条 Render 命令的 frame_id，供渲染完成后通知 UI
        let latest_frame_id = 0u64;
        let textures = LoopTextures {
            current: None,
            depth: None,
            depth_view: None,
            view: None,
        };
        let current_size = (0, 0);
        let last_note_version = 0;

        // 高精度贴图瀑布流渲染器状态
        let waterfall = LoopWaterfall {
            renderer: None,
            meta: None,
            config: None,
            // 视频导出专用 GPU 渲染器（跨帧复用，避免每帧重建 pipeline）
            video_renderer: None,
            miditrail_renderer: None,
        };
        let deferred = Vec::new();

        // 视频导出读回管线状态
        let export = LoopExport {
            pipeline: None,
            frame_tx: None,
        };

        // 贴图瀑布流流式上传状态：true 表示正在接收 chunk（已 begin_streaming_upload）
        let streaming_in_progress = false;
        // 贴图瀑布流 GPU 布局段表（全量会话构建，增量替换时更新）
        let segments = Vec::new();
        // 当前音轨编码（track_idx+1，0=无）：统一全量渲染的视图状态（切轨零重传）
        let current_track_encoded = 0;

        Self {
            renderers,
            export_renderers,
            frame_count,
            fps_update_time,
            latest_frame_id,
            textures,
            current_size,
            last_note_version,
            waterfall,
            deferred,
            export,
            onion: LoopOnion {
                streaming_in_progress,
                segments,
                current_track_encoded,
            },
        }
    }

    /// 收命令：drain 控制/Render 命令队列，产出本轮参数与关停意图。
    fn collect_commands(
        &mut self,
        channels: &RenderThreadChannels,
    ) -> (bool, bool, Option<RenderParams>) {
        // 处理命令
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;
        let has_params = process_commands(
            &channels.command_receiver,
            &mut latest_params,
            &mut self.latest_frame_id,
            &mut should_shutdown,
            &mut self.deferred,
        );
        (has_params, should_shutdown, latest_params)
    }

    /// 应用状态：延迟命令、导出 inflight、两路流式上传。
    fn apply_deferred_and_streams(
        &mut self,
        ctx: &RenderContext,
        channels: &RenderThreadChannels,
        texture_waterfall_result_tx: &std::sync::mpsc::SyncSender<WaterfallStreamMsg>,
        texture_waterfall_result_rx: &std::sync::mpsc::Receiver<WaterfallStreamMsg>,
    ) {
        // 处理延迟的控制命令
        process_deferred_commands(
            &mut DeferredCommandContext {
                ctx,
                channels,
                renderers: &mut self.renderers,
                export_renderers: &mut self.export_renderers,
                current_texture: &mut self.textures.current,
                depth_texture: &mut self.textures.depth,
                depth_texture_view: &mut self.textures.depth_view,
                texture_view: &mut self.textures.view,
                current_size: &mut self.current_size,
                last_note_version: &mut self.last_note_version,
                texture_waterfall_renderer: &mut self.waterfall.renderer,
                texture_waterfall_meta: &mut self.waterfall.meta,
                texture_waterfall_config: &mut self.waterfall.config,
                export_pipeline: &mut self.export.pipeline,
                export_frame_tx: &mut self.export.frame_tx,
                waterfall_renderer: &mut self.waterfall.video_renderer,
                miditrail_renderer: &mut self.waterfall.miditrail_renderer,
                texture_waterfall_result_tx,
                onion_streaming_in_progress: self.onion.streaming_in_progress,
            },
            &mut self.deferred,
        );

        // 推进视频导出 inflight：即使没有新的 RenderVideoFrame 命令，
        // 也需要 try_read 已就绪的帧数据并发回 Runner，否则 inflight 满后
        // Runner 阻塞在 frame_rx.recv()，渲染线程也不再调用 try_read，形成死锁。
        advance_export_inflight(&mut self.export.pipeline, &self.export.frame_tx);

        // ★ 流式接收：每帧循环 try_recv，收到已合并像素立即 upload（GPU DMA，非阻塞）★
        let gpu = WaterfallGpuCtx {
            device: &ctx.device,
            queue: &ctx.queue,
            texture_format: ctx.texture_format,
        };
        drain_waterfall_stream(
            texture_waterfall_result_rx,
            &gpu,
            &mut self.waterfall.renderer,
            &channels.waterfall_progress,
        );

        // ★ 贴图瀑布流流式上传：drain channel，逐块 streaming_append 到 GPU ★
        drain_onion_skin_stream(
            ctx,
            &mut self.renderers,
            &mut self.onion.segments,
            &mut self.onion.streaming_in_progress,
            &mut self.onion.current_track_encoded,
            &channels.onion_skin_streaming_rx,
        );
    }

    /// 应用主音轨事件级增量（段内应用）。
    fn apply_main_track_events(&mut self, ctx: &RenderContext, channels: &RenderThreadChannels) {
        // ★ 主音轨事件级增量（段内应用）：drain note_events_rx → 当前音轨段
        // GPU 布局 = 全量轨段，事件 index = notes 索引（保序，无需可见列表映射）。
        // 段表定位依赖 SetViewState（切轨消息）先于编辑事件到达（mpsc 顺序保证）。
        if self.onion.current_track_encoded != 0 {
            let updated = process_main_track_events(
                &mut self.renderers,
                &mut self.onion.segments,
                self.onion.current_track_encoded,
                &channels.note_events_rx,
                &ctx.device,
                &ctx.queue,
            );
            if updated {
                self.renderers
                    .onion_skin
                    .update_cull_info(&ctx.device, &ctx.queue);
                let total_gpu_mb =
                    lumino_diagnostics::memtrace::Snapshot::capture().total_with_gpu_mb();
                tracing::debug!(
                    "MainTrack: 事件应用后 instance_count={} instance_buf={}MB visible_index_buf={}MB total_gpu={:.1}MB",
                    self.renderers.onion_skin.gpu_instance_count(),
                    self.renderers.onion_skin.instance_buffer_size() / 1024 / 1024,
                    self.renderers.onion_skin.visible_buffer_size() / 1024 / 1024,
                    total_gpu_mb
                );
            }
        }
    }

    /// 准备帧 + 编码提交 + 统计（离屏纹理渲染）。
    fn render_frame(
        &mut self,
        ctx: &RenderContext,
        channels: &RenderThreadChannels,
        params: &mut RenderParams,
    ) {
        puffin::profile_scope!("wgpu_render_thread_frame");
        let frame_start = Instant::now();

        // 走带音符层：复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存）。
        // 依据侧栏音轨顺序预计算 lane 映射 / uniform，写入 params，
        // 供 prepare_renderers 直接驱动走带音符 GPU 裁剪 + draw_indirect 管线。
        if params.is_arrangement_mode {
            prepare_arrangement_note_data(params);
        }

        // 全屏瀑布流播放器模式：与钢琴卷帘完全隔离，跳过卷帘 3D 场景绘制
        // （网格/音符/洋葱皮的离屏绘制，最贵的 GPU 工作），解放 GPU。
        // 下方仍照常发布活体音符缓冲（note_data_pub）供播放器复用，禁止第二份拷贝。
        if !params.skip_scene_render {
            ensure_offscreen_textures_and_upload_notes(&mut PreviewUploadContext {
                ctx,
                channels,
                current_texture: &mut self.textures.current,
                depth_texture: &mut self.textures.depth,
                depth_texture_view: &mut self.textures.depth_view,
                texture_view: &mut self.textures.view,
                current_size: &mut self.current_size,
                params,
            });

            render_offscreen_pass(&mut PreviewPassContext {
                ctx,
                params,
                channels,
                renderers: &mut self.renderers,
                current_texture: &mut self.textures.current,
                depth_texture: &mut self.textures.depth,
                depth_texture_view: &mut self.textures.depth_view,
                texture_view: &mut self.textures.view,
                current_size: &mut self.current_size,
                last_note_version: &mut self.last_note_version,
                texture_waterfall_renderer: &mut self.waterfall.renderer,
                texture_waterfall_meta: &mut self.waterfall.meta,
                texture_waterfall_config: &mut self.waterfall.config,
                export_pipeline: &mut self.export.pipeline,
                export_frame_tx: &mut self.export.frame_tx,
            });
        }

        // 发布活体音符实例缓冲给 UI 线程（侧边瀑布流面板复用，禁止第二份拷贝）
        {
            let (buf, count) = self.renderers.onion_skin.gpu_note_buffer_for_sharing();
            if let Ok(mut guard) = channels.note_data_pub.lock() {
                *guard = Some((buf, count));
            }
        }

        // Tracy 帧视图（交互渲染；导出帧由 `handle_video_frame` 单独标记）
        crate::profiling::frame_mark();
        crate::profiling::tracy_plot!(
            "gfx.interactive_frame_ms",
            frame_start.elapsed().as_secs_f64() * 1000.0
        );

        // 更新统计
        let frame_time = frame_start.elapsed();
        update_stats(
            &mut self.frame_count,
            &mut self.fps_update_time,
            frame_time,
            &channels.stats_clone,
            self.renderers.ruler.instance_count(),
        );

        self.notify_frame_done(channels);
    }

    fn notify_frame_done(&self, channels: &RenderThreadChannels) {
        // 通知 UI 线程：本帧（frame_id）已渲染完成（**提交序**，非 GPU 执行完成），
        // 可安全 present（copy 到 Surface）。修复音符放置后不立即显示：UI 线程
        // present 前需 wait_for_frame，确保拷到含本次编辑（如音符 Insert）的最新
        // 离屏帧，而非尚未被本线程处理的旧帧。
        let (mtx, cvar) = &*channels.frame_sync;
        if let Ok(mut guard) = mtx.lock() {
            *guard = self.latest_frame_id;
            cvar.notify_all();
        }
    }

    fn run_loop(
        &mut self,
        ctx: &RenderContext,
        channels: &RenderThreadChannels,
        texture_waterfall_result_tx: &std::sync::mpsc::SyncSender<WaterfallStreamMsg>,
        texture_waterfall_result_rx: &std::sync::mpsc::Receiver<WaterfallStreamMsg>,
    ) {
        while channels.running.load(Ordering::Relaxed) {
            // REND-019（PR-A §1.6）：每轮首行检查停帧标志——设备丢失（TDR）后
            // 不再对死设备发命令，有序退出（资源随作用域统一释放；running 置
            // false 供外部感知；重连接管由 PR-C 接管）。
            if crate::device_status::device_lost() {
                tracing::error!("DEVICE_LOST 置位：渲染线程有序退出（等待重连接管）");
                channels.running.store(false, Ordering::Relaxed);
                break;
            }

            let (has_params, should_shutdown, mut latest_params) = self.collect_commands(channels);
            self.apply_deferred_and_streams(
                ctx,
                channels,
                texture_waterfall_result_tx,
                texture_waterfall_result_rx,
            );

            if should_shutdown {
                break;
            }

            self.apply_main_track_events(ctx, channels);

            // 执行渲染（离屏纹理）
            if has_params && let Some(ref mut params) = latest_params {
                self.render_frame(ctx, channels, params);
            }
        }
    }
}

/// 渲染线程主循环实现（由 [`run_render_thread`] 以兜底方式调用）。
fn run_render_thread_inner(ctx: RenderContext, channels: RenderThreadChannels) {
    tracing::info!("Render thread started");

    // REND-019 PR-B：本线程持有的全部 GPU 资源（渲染器 [Renderers.gpu_epoch] /
    // 离屏纹理 / 导出管线 / 瀑布流渲染器）以 `ctx.epoch` 为统一登记基准——
    // 跨代次即整体判废、重建线程（PR-C）；线程内私有句柄不逐对象登记
    // （由线程生命周期统辖，E2 重建路径 = 线程重启）。
    let mut state = LoopState::new(&ctx);

    // ★ 后台生成线程通过有界同步通道流式传回贴图（容量1，背压）★
    // sync_channel(1)：channel 满时 send 阻塞，强制后台等渲染线程消费，
    // 防止无界积压导致 CPU 内存峰值（对应"装袋期间工人等着"）
    let (texture_waterfall_result_tx, texture_waterfall_result_rx) =
        std::sync::mpsc::sync_channel::<WaterfallStreamMsg>(1);

    state.run_loop(
        &ctx,
        &channels,
        &texture_waterfall_result_tx,
        &texture_waterfall_result_rx,
    );
}
