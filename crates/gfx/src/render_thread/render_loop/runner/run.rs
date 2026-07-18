use std::sync::{Arc, Mutex, atomic::Ordering};
use std::time::Instant;

use super::super::super::commands::{ControlCommand, FrameSender};
use super::super::super::export_pipeline::ExportPipeline;
use super::super::super::params::RenderParams;
use super::super::commands::process_commands;
use super::super::prepare::prepare_renderers;
use super::super::render_pass::execute_render_pass;
use super::super::render_pass::update_stats;
use super::super::textures::ensure_textures;

use super::context::{
    RenderContext, RenderFrameState, RenderThreadChannels, UploadHiResTileParams,
};
use super::hires::{
    handle_hires_control, push_onion_progress, update_hires_viewport, upload_hires_video_tiles,
};
use super::types::HiResStreamMsg;

/// 运行渲染线程主循环
pub fn run_render_thread(ctx: RenderContext, channels: RenderThreadChannels) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut renderers = super::super::Renderers {
        grid: crate::GridRenderer::new(&ctx.device, ctx.texture_format),
        note: crate::NoteRenderer::new(&ctx.device, &ctx.queue, ctx.texture_format),
        ruler: crate::RulerRenderer::new(&ctx.device, ctx.texture_format),
        arrangement: crate::ArrangementRenderer::new(&ctx.device, ctx.texture_format),
        cc_bar: crate::CcBarRenderer::new(&ctx.device, ctx.texture_format),
    };

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);
    let mut last_note_version: u64 = 0;

    // 高精度洋葱皮渲染器状态
    let mut hires_renderer: Option<crate::HiResRenderer> = None;
    let mut hires_meta = None;
    let mut hires_config = None;
    let mut deferred: Vec<ControlCommand> = Vec::new();

    // 视频导出读回管线状态
    let mut export_pipeline: Option<ExportPipeline> = None;
    let mut export_frame_tx = None;

    // ★ 后台生成线程通过有界同步通道流式传回贴图（容量1，背压）★
    // sync_channel(1)：channel 满时 send 阻塞，强制后台等渲染线程消费，
    // 防止无界积压导致 CPU 内存峰值（对应"装袋期间工人等着"）
    let (hires_result_tx, hires_result_rx) = std::sync::mpsc::sync_channel::<HiResStreamMsg>(1);

    while channels.running.load(Ordering::Relaxed) {
        // 处理命令
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        let has_params = process_commands(
            &channels.command_receiver,
            &mut latest_params,
            &mut should_shutdown,
            &mut deferred,
        );

        // 处理延迟的控制命令
        process_deferred_commands(
            &ctx,
            &channels,
            &mut renderers,
            &mut current_texture,
            &mut depth_texture,
            &mut depth_texture_view,
            &mut current_size,
            &mut last_note_version,
            &mut hires_renderer,
            &mut hires_meta,
            &mut hires_config,
            &mut export_pipeline,
            &mut export_frame_tx,
            &hires_result_tx,
            &mut deferred,
        );

        // 推进视频导出 inflight：即使没有新的 RenderVideoFrame 命令，
        // 也需要 try_read 已就绪的帧数据并发回 Runner，否则 inflight 满后
        // Runner 阻塞在 frame_rx.recv()，渲染线程也不再调用 try_read，形成死锁。
        advance_export_inflight(&mut export_pipeline, &export_frame_tx);

        // ★ 流式接收：每帧循环 try_recv，收到已合并像素立即 upload（GPU DMA，非阻塞）★
        drain_hires_stream(
            &hires_result_rx,
            &ctx,
            &mut hires_renderer,
            &channels.onion_progress,
        );

        if should_shutdown {
            break;
        }

        // 执行渲染（离屏纹理）
        if has_params && let Some(ref params) = latest_params {
            puffin::profile_scope!("wgpu_render_thread_frame");
            let frame_start = Instant::now();

            ensure_offscreen_textures_and_upload_notes(
                &ctx,
                &channels,
                &mut renderers,
                &mut current_texture,
                &mut depth_texture,
                &mut depth_texture_view,
                &mut current_size,
                &mut last_note_version,
                params,
            );

            render_offscreen_pass(
                &ctx,
                params,
                &channels,
                &mut renderers,
                &mut current_texture,
                &mut depth_texture,
                &mut depth_texture_view,
                &mut current_size,
                &mut last_note_version,
                &mut hires_renderer,
                &mut hires_meta,
                &mut hires_config,
                &mut export_pipeline,
                &mut export_frame_tx,
            );

            // 更新统计
            let frame_time = frame_start.elapsed();
            update_stats(
                &mut frame_count,
                &mut fps_update_time,
                frame_time,
                params,
                &channels.stats_clone,
            );
        }
    }

    tracing::info!("Render thread stopped");
}

/// 流式接收后台生成的高精度贴图并上传到 GPU。
///
/// 每帧循环 `try_recv`，收到已合并像素立即 `upload_tile`（GPU DMA，非阻塞）；
/// 收到 `Finished` 后 flush DMA 并推送完成进度。无更多消息即退出本帧接收。
fn drain_hires_stream(
    hires_result_rx: &std::sync::mpsc::Receiver<HiResStreamMsg>,
    ctx: &RenderContext,
    hires_renderer: &mut Option<crate::HiResRenderer>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    loop {
        match hires_result_rx.try_recv() {
            Ok(HiResStreamMsg::TimeGroupMerged {
                track_group,
                time_group,
                pixels,
                width,
                height,
            }) => {
                // ★ merge 已在后台线程完成，渲染线程仅做 GPU 上传（DMA 异步，非阻塞）★
                tracing::debug!(
                    "[onion-render] 收到并上传整合组贴图: track_group={}, time_group={}, pixels={}",
                    track_group,
                    time_group,
                    pixels.len()
                );
                if let Some(renderer) = hires_renderer {
                    let coord = crate::TileCoord::new(track_group, time_group);
                    renderer.upload_tile(&ctx.device, &ctx.queue, coord, &pixels, width, height);
                }
                // pixels 在此 drop，释放 CPU 像素缓冲
            }
            Ok(HiResStreamMsg::Finished) => {
                // 后台生成全部完毕：flush DMA
                if hires_renderer.is_some() {
                    let flush =
                        ctx.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("hires_stream_flush"),
                            });
                    ctx.queue.submit(std::iter::once(flush.finish()));
                }
                push_onion_progress(onion_progress, "高精度洋葱皮贴图流式生成+上传完成", 1.0);
            }
            Err(_) => break, // 无更多消息，退出本帧接收
        }
    }
}

/// 推进视频导出 inflight 帧读回。
///
/// 即使没有新的 `RenderVideoFrame` 命令，也需要 `try_read` 已就绪的帧数据并发回 Runner，
/// 否则 inflight 满后 Runner 阻塞在 `frame_rx.recv()`，渲染线程也不再调用 `try_read`，形成死锁。
fn advance_export_inflight(
    export_pipeline: &mut Option<ExportPipeline>,
    export_frame_tx: &Option<FrameSender>,
) {
    if let (Some(pipeline), Some(tx)) = (export_pipeline, export_frame_tx) {
        while let Some(data) = pipeline.try_read() {
            if tx.0.send(data).is_err() {
                tracing::warn!("视频帧发送失败：Runner 通道已关闭");
                break;
            }
        }
    }
}

/// 处理视频导出帧：离屏渲染 → copy 到 staging → submit → map_async
///
/// 流水线模式：不阻塞等待当前帧读回，而是利用四重缓冲让 GPU 渲染与 CPU 读回重叠。
/// - inflight 达到上限（4）时，阻塞读最早的一帧（此时 GPU 通常已完成 map_async）
/// - copy_and_submit 后立即返回，GPU 继续处理下一帧
/// - try_read 非阻塞读回已就绪的帧
///
/// 这会打破"每命令一帧"的语义：Runner 发 N 帧命令可能先收到 0~4 帧数据，
/// 剩余帧在 FinishVideoExport 或后续命令中读回。Runner 侧需用 param_queue 跟踪。
fn handle_video_frame(
    ctx: &RenderContext,
    params: RenderParams,
    frame: &mut RenderFrameState,
    channels: &RenderThreadChannels,
) {
    // 提前检查导出管线是否已初始化，避免后续重复判断
    let pipeline_ready = frame.export_pipeline.is_some() && frame.export_frame_tx.is_some();
    if !pipeline_ready {
        tracing::warn!("RenderVideoFrame 收到但导出管线未初始化，忽略");
        return;
    }

    let width = params.viewport_size.0.max(1);
    let height = params.viewport_size.1.max(1);

    // 1. 确保离屏纹理已创建
    let mut tex_resources = super::super::textures::OffscreenTextureResources {
        device: &ctx.device,
        texture_format: ctx.texture_format,
        width,
        height,
        current_size: frame.current_size,
        current_texture: frame.current_texture,
        depth_texture: frame.depth_texture,
        depth_texture_view: frame.depth_texture_view,
        latest_texture_clone: frame.latest_texture_clone,
        params: &params,
    };
    ensure_textures(&mut tex_resources);

    // 视频导出始终使用音符矩形渲染模式：不上传 HiRes 贴图
    let hires_visible_coords: Vec<crate::TileCoord> = Vec::new();

    // 2. 上传视频导出帧的音符实例
    if !params.note_instances.is_empty() {
        frame
            .renderers
            .note
            .upload_instances(&params.note_instances, &ctx.device, &ctx.queue);
    }

    // 3. 检查离屏纹理是否就绪（clone Arc 断开与 frame 的借用链，
    //    使后续 execute_render_pass 可以再借 &mut frame）。
    let texture_opt = frame.current_texture.as_ref().map(Arc::clone);
    let depth_ready = frame.depth_texture_view.is_some();
    let (Some(texture_arc), true) = (texture_opt, depth_ready) else {
        tracing::warn!("视频帧渲染：离屏纹理未就绪，跳过");
        return;
    };

    // 4. 创建编码器并执行渲染通道
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("video_export_render_encoder"),
        });

    prepare_renderers(
        frame.renderers,
        &params,
        &channels.note_events_rx,
        &ctx.device,
        &ctx.queue,
    );

    execute_render_pass(
        &mut encoder,
        ctx,
        &params,
        &hires_visible_coords,
        true,
        frame,
    );

    // 5. 流水线模式：inflight 达到上限时阻塞读最早的一帧（背压）
    // 此时从 frame 获取 pipeline / tx 引用，execute_render_pass 已经完成，
    // 不会再与 frame 的借用冲突。
    // pipeline_ready 已提前保证 export_pipeline / export_frame_tx 不为 None。
    let pipeline = frame
        .export_pipeline
        .as_mut()
        .expect("export_pipeline 应已通过 pipeline_ready 检查完成初始化，不应为 None");
    let tx = frame
        .export_frame_tx
        .as_ref()
        .expect("export_frame_tx 应已通过 pipeline_ready 检查完成初始化，不应为 None");
    pipeline.ensure_size(width, height);
    while !pipeline.can_write() {
        let data = pipeline.wait_read();
        if tx.0.send(data).is_err() {
            tracing::warn!("视频帧发送失败：Runner 通道已关闭");
            return;
        }
    }

    // 6. copy 离屏纹理到 staging buffer + submit + map_async（非阻塞，立即返回）
    pipeline.copy_and_submit(encoder, &texture_arc, &ctx.queue);

    // 6. 尝试非阻塞读回已就绪的帧（流水线推进，不阻塞下一帧渲染）
    while let Some(data) = pipeline.try_read() {
        if tx.0.send(data).is_err() {
            tracing::warn!("视频帧发送失败：Runner 通道已关闭");
            return;
        }
    }
}

/// 处理延迟队列中的控制命令（视频导出 / 高精度贴图 / HiRes 控制）。
fn process_deferred_commands(
    ctx: &RenderContext,
    channels: &RenderThreadChannels,
    renderers: &mut super::super::Renderers,
    current_texture: &mut Option<Arc<wgpu::Texture>>,
    depth_texture: &mut Option<wgpu::Texture>,
    depth_texture_view: &mut Option<wgpu::TextureView>,
    current_size: &mut (u32, u32),
    last_note_version: &mut u64,
    hires_renderer: &mut Option<crate::HiResRenderer>,
    hires_meta: &mut Option<super::types::HiResMeta>,
    hires_config: &mut Option<crate::HiResConfig>,
    export_pipeline: &mut Option<ExportPipeline>,
    export_frame_tx: &mut Option<FrameSender>,
    hires_result_tx: &std::sync::mpsc::SyncSender<HiResStreamMsg>,
    deferred: &mut Vec<ControlCommand>,
) {
    for cmd in deferred.drain(..) {
        match cmd {
            // ── 视频导出命令：在此内联处理（需要 GPU 资源 + 离屏纹理）──
            ControlCommand::StartVideoExport {
                width,
                height,
                frame_tx,
            } => {
                tracing::info!(
                    "视频导出开始: {}x{}, 初始化 GPU→CPU 读回管线",
                    width,
                    height,
                );
                *export_pipeline = Some(ExportPipeline::new(&ctx.device, width, height));
                *export_frame_tx = Some(frame_tx);
            }
            ControlCommand::RenderVideoFrame { params } => {
                let mut frame = RenderFrameState {
                    renderers: &mut *renderers,
                    current_texture: &mut *current_texture,
                    depth_texture: &mut *depth_texture,
                    depth_texture_view: &mut *depth_texture_view,
                    current_size: &mut *current_size,
                    last_note_version: &mut *last_note_version,
                    latest_texture_clone: &channels.latest_texture_clone,
                    hires_renderer: &mut *hires_renderer,
                    hires_meta: &mut *hires_meta,
                    hires_config: &mut *hires_config,
                    export_pipeline: &mut *export_pipeline,
                    export_frame_tx: &mut *export_frame_tx,
                };
                handle_video_frame(ctx, *params, &mut frame, channels);
            }
            ControlCommand::UploadHiResVideoTiles {
                tiles,
                config,
                track_count,
                key_count,
                total_ticks,
                ppq,
            } => {
                let params = UploadHiResTileParams {
                    tiles,
                    config,
                    track_count,
                    key_count,
                    total_ticks,
                    ppq,
                };
                upload_hires_video_tiles(
                    ctx,
                    &mut *hires_renderer,
                    &mut *hires_meta,
                    &mut *hires_config,
                    params,
                );
            }
            ControlCommand::FinishVideoExport => {
                tracing::info!("视频导出完成，释放读回管线");
                *export_pipeline = None;
                *export_frame_tx = None;
            }
            // ── HiRes 命令走原路径 ──
            cmd => {
                handle_hires_control(
                    cmd,
                    ctx,
                    hires_result_tx,
                    &channels.onion_progress,
                    &mut *hires_renderer,
                    &mut *hires_meta,
                    &mut *hires_config,
                );
            }
        }
    }
}

/// 确保离屏纹理已创建，并在主音符实例版本变化时上传。
fn ensure_offscreen_textures_and_upload_notes(
    ctx: &RenderContext,
    channels: &RenderThreadChannels,
    _renderers: &mut super::super::Renderers,
    current_texture: &mut Option<Arc<wgpu::Texture>>,
    depth_texture: &mut Option<wgpu::Texture>,
    depth_texture_view: &mut Option<wgpu::TextureView>,
    current_size: &mut (u32, u32),
    last_note_version: &mut u64,
    params: &RenderParams,
) {
    let width = params.viewport_size.0.max(1);
    let height = params.viewport_size.1.max(1);

    let mut tex_resources = super::super::textures::OffscreenTextureResources {
        device: &ctx.device,
        texture_format: ctx.texture_format,
        width,
        height,
        current_size: &mut *current_size,
        current_texture: &mut *current_texture,
        depth_texture: &mut *depth_texture,
        depth_texture_view: &mut *depth_texture_view,
        latest_texture_clone: &channels.latest_texture_clone,
        params,
    };
    ensure_textures(&mut tex_resources);

    let note_version = channels.note_instances_buffer.version();
    if note_version != *last_note_version {
        *last_note_version = note_version;

        // 依赖事件通道（process_events 在 prepare_renderers 中调用）做增量更新，
        // 而非全量重传。事件通道通过 add_note / update_note / remove_note 做 O(1) 更新，
        // 避免 160MB 全量 write_buffer 的帧瞬卡。
        //
        // 但仍需 acquire_read_buffer 以维持三缓冲状态机正常运转（ready ↔ reading 交换）。
        // 若 UI 线程 swap 后 reading 槽位未被消费，后续 swap 会反复交换同一槽位。
        let _ = unsafe { channels.note_instances_buffer.acquire_read_buffer() };
    }
}

/// 执行离屏渲染通道（在离屏纹理就绪时）：准备渲染器、更新高精度视口、提交命令。
fn render_offscreen_pass(
    ctx: &RenderContext,
    params: &RenderParams,
    channels: &RenderThreadChannels,
    renderers: &mut super::super::Renderers,
    current_texture: &mut Option<Arc<wgpu::Texture>>,
    depth_texture: &mut Option<wgpu::Texture>,
    depth_texture_view: &mut Option<wgpu::TextureView>,
    current_size: &mut (u32, u32),
    last_note_version: &mut u64,
    hires_renderer: &mut Option<crate::HiResRenderer>,
    hires_meta: &mut Option<super::types::HiResMeta>,
    hires_config: &mut Option<crate::HiResConfig>,
    export_pipeline: &mut Option<ExportPipeline>,
    export_frame_tx: &mut Option<FrameSender>,
) {
    if let (Some(_texture), Some(_depth_view)) = (&*current_texture, &*depth_texture_view) {
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("offscreen_render_encoder"),
            });

        prepare_renderers(
            &mut *renderers,
            params,
            &channels.note_events_rx,
            &ctx.device,
            &ctx.queue,
        );

        let hires_visible = update_hires_viewport(
            &mut *hires_renderer,
            &*hires_meta,
            &*hires_config,
            params,
            &ctx.device,
            &ctx.queue,
        );
        let hires_visible_coords: Vec<crate::TileCoord> =
            hires_visible.iter().map(|(c, _)| *c).collect();

        let mut frame = RenderFrameState {
            renderers,
            current_texture,
            depth_texture,
            depth_texture_view,
            current_size,
            last_note_version,
            latest_texture_clone: &channels.latest_texture_clone,
            hires_renderer,
            hires_meta,
            hires_config,
            export_pipeline,
            export_frame_tx,
        };
        execute_render_pass(
            &mut encoder,
            ctx,
            params,
            &hires_visible_coords,
            true,
            &mut frame,
        );

        ctx.queue.submit(std::iter::once(encoder.finish()));
    }
}
