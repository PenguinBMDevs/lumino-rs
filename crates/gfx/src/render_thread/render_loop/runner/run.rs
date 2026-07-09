use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::SwappableBuffer;

use super::super::super::commands::{ControlCommand, FrameSender, RenderCommand};
use super::super::super::export_pipeline::ExportPipeline;
use super::super::super::params::RenderParams;
use super::super::super::stats::RenderStats;
use super::super::Renderers;
use super::super::commands::process_commands;
use super::super::prepare::prepare_renderers;
use super::super::render_pass::execute_render_pass;
use super::super::render_pass::update_stats;
use super::super::textures::ensure_textures;

use super::hires::{handle_hires_control, push_onion_progress, update_hires_viewport};
use super::types::{HiResMeta, HiResStreamMsg};

/// 运行渲染线程主循环
#[allow(clippy::too_many_arguments)]
pub fn run_render_thread(
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    running: Arc<AtomicBool>,
    command_receiver: std::sync::mpsc::Receiver<RenderCommand>,
    latest_texture_clone: Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    stats_clone: Arc<Mutex<RenderStats>>,
    note_events_rx: std::sync::mpsc::Receiver<crate::NoteEvent>,
    note_instances_buffer: Arc<SwappableBuffer<crate::NoteInstance>>,
    onion_progress: Arc<Mutex<Vec<(String, f32)>>>,
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut renderers = Renderers {
        grid: crate::GridRenderer::new(&device, texture_format),
        note: crate::NoteRenderer::new(&device, &queue, texture_format),
        ruler: crate::RulerRenderer::new(&device, texture_format),
        arrangement: crate::ArrangementRenderer::new(&device, texture_format),
        cc_bar: crate::CcBarRenderer::new(&device, texture_format),
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
    let mut hires_meta: Option<HiResMeta> = None;
    let mut hires_config: Option<crate::HiResConfig> = None;
    let mut deferred: Vec<ControlCommand> = Vec::new();

    // 视频导出读回管线状态
    let mut export_pipeline: Option<ExportPipeline> = None;
    let mut export_frame_tx: Option<FrameSender> = None;

    // ★ 后台生成线程通过有界同步通道流式传回贴图（容量1，背压）★
    // sync_channel(1)：channel 满时 send 阻塞，强制后台等渲染线程消费，
    // 防止无界积压导致 CPU 内存峰值（对应"装袋期间工人等着"）
    let (hires_result_tx, hires_result_rx) = std::sync::mpsc::sync_channel::<HiResStreamMsg>(1);

    while running.load(Ordering::Relaxed) {
        // 处理命令
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        let has_params = process_commands(
            &command_receiver,
            &mut latest_params,
            &mut should_shutdown,
            &mut deferred,
        );

        // 处理延迟的控制命令
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
                        height
                    );
                    export_pipeline = Some(ExportPipeline::new(&device, width, height));
                    export_frame_tx = Some(frame_tx);
                }
                ControlCommand::RenderVideoFrame(params) => {
                    handle_video_frame(
                        *params,
                        &device,
                        &queue,
                        texture_format,
                        &mut export_pipeline,
                        &export_frame_tx,
                        &mut renderers,
                        &mut current_texture,
                        &mut depth_texture,
                        &mut depth_texture_view,
                        &mut current_size,
                        &note_instances_buffer,
                        &note_events_rx,
                        &mut last_note_version,
                        &latest_texture_clone,
                        &mut hires_renderer,
                        &mut hires_meta,
                        &mut hires_config,
                    );
                }
                ControlCommand::FinishVideoExport => {
                    tracing::info!("视频导出完成，释放读回管线");
                    export_pipeline = None;
                    export_frame_tx = None;
                }
                // ── HiRes 命令走原路径 ──
                cmd => {
                    handle_hires_control(
                        cmd,
                        &device,
                        &queue,
                        &mut hires_renderer,
                        &mut hires_meta,
                        &mut hires_config,
                        &hires_result_tx,
                        &onion_progress,
                        texture_format,
                    );
                }
            }
        }

        // ★ 流式接收：每帧循环 try_recv，收到已合并像素立即 upload（GPU DMA，非阻塞）★
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
                    if let Some(renderer) = &mut hires_renderer {
                        let coord = crate::TileCoord::new(track_group, time_group);
                        renderer.upload_tile(&device, &queue, coord, &pixels, width, height);
                    }
                    // pixels 在此 drop，释放 CPU 像素缓冲
                }
                Ok(HiResStreamMsg::Finished) => {
                    // 后台生成全部完毕：flush DMA
                    if hires_renderer.is_some() {
                        let flush =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("hires_stream_flush"),
                            });
                        queue.submit(std::iter::once(flush.finish()));
                    }
                    push_onion_progress(&onion_progress, "高精度洋葱皮贴图流式生成+上传完成", 1.0);
                }
                Err(_) => break, // 无更多消息，退出本帧接收
            }
        }

        if should_shutdown {
            break;
        }

        // 执行渲染（离屏纹理）
        if has_params && let Some(ref params) = latest_params {
            puffin::profile_scope!("wgpu_render_thread_frame");
            let frame_start = Instant::now();

            let width = params.viewport_size.0.max(1);
            let height = params.viewport_size.1.max(1);

            // 确保离屏纹理已创建
            let mut tex_resources = super::super::textures::OffscreenTextureResources {
                device: &device,
                texture_format,
                width,
                height,
                current_size: &mut current_size,
                current_texture: &mut current_texture,
                depth_texture: &mut depth_texture,
                depth_texture_view: &mut depth_texture_view,
                latest_texture_clone: &latest_texture_clone,
                params,
            };
            ensure_textures(&mut tex_resources);

            // 仅检测主音符版本号变化后上传
            let note_version = note_instances_buffer.version();
            if note_version != last_note_version {
                last_note_version = note_version;

                puffin::profile_scope!("upload_note_instances_from_buffer");
                let notes = unsafe { note_instances_buffer.read_buffer() };

                renderers.note.upload_instances(notes, &device, &queue);
            }

            if let (Some(_texture), Some(_depth_view)) = (&current_texture, &depth_texture_view) {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("offscreen_render_encoder"),
                });

                // 准备渲染器
                prepare_renderers(&mut renderers, params, &note_events_rx, &device, &queue);

                // 高精度贴图视口驱动
                let hires_visible = update_hires_viewport(
                    &mut hires_renderer,
                    &hires_meta,
                    &hires_config,
                    params,
                    &device,
                    &queue,
                );
                let hires_visible_coords: Vec<crate::TileCoord> =
                    hires_visible.iter().map(|(c, _)| *c).collect();

                // 执行渲染通道
                execute_render_pass(
                    &mut encoder,
                    &device,
                    &current_texture,
                    &depth_texture_view,
                    params,
                    &mut renderers.grid,
                    &mut renderers.note,
                    &mut renderers.ruler,
                    &mut renderers.arrangement,
                    &queue,
                    &mut renderers.cc_bar,
                    &hires_renderer,
                    &hires_visible_coords,
                );

                // 提交渲染指令
                queue.submit(std::iter::once(encoder.finish()));
            }

            // 更新统计
            let frame_time = frame_start.elapsed();
            update_stats(
                &mut frame_count,
                &mut fps_update_time,
                frame_time,
                params,
                &stats_clone,
            );
        }
    }

    tracing::info!("Render thread stopped");
}

/// 处理视频导出帧：离屏渲染 → copy 到 staging → submit → map_async → wait_read → send
///
/// 同步模式：阻塞等待当前帧读回后再发送给 Runner。
/// 视频导出为非实时任务，同步模式代码简洁且足够可靠。
#[allow(clippy::too_many_arguments)]
fn handle_video_frame(
    params: RenderParams,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    export_pipeline: &mut Option<ExportPipeline>,
    export_frame_tx: &Option<FrameSender>,
    renderers: &mut super::super::Renderers,
    current_texture: &mut Option<Arc<wgpu::Texture>>,
    depth_texture: &mut Option<wgpu::Texture>,
    depth_texture_view: &mut Option<wgpu::TextureView>,
    current_size: &mut (u32, u32),
    note_instances_buffer: &Arc<SwappableBuffer<crate::NoteInstance>>,
    note_events_rx: &std::sync::mpsc::Receiver<crate::NoteEvent>,
    last_note_version: &mut u64,
    latest_texture_clone: &Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    hires_renderer: &mut Option<crate::HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<crate::HiResConfig>,
) {
    let (Some(pipeline), Some(tx)) = (export_pipeline.as_mut(), export_frame_tx) else {
        tracing::warn!("RenderVideoFrame 收到但导出管线未初始化，忽略");
        return;
    };

    let width = params.viewport_size.0.max(1);
    let height = params.viewport_size.1.max(1);

    // 1. 确保离屏纹理已创建
    let mut tex_resources = super::super::textures::OffscreenTextureResources {
        device,
        texture_format,
        width,
        height,
        current_size,
        current_texture,
        depth_texture,
        depth_texture_view,
        latest_texture_clone,
        params: &params,
    };
    ensure_textures(&mut tex_resources);

    // 2. 音符版本检测后上传
    let note_version = note_instances_buffer.version();
    if note_version != *last_note_version {
        *last_note_version = note_version;
        let notes = unsafe { note_instances_buffer.read_buffer() };
        renderers.note.upload_instances(notes, device, queue);
    }

    let (Some(texture), Some(_depth_view)) = (&*current_texture, &*depth_texture_view) else {
        tracing::warn!("视频帧渲染：离屏纹理未就绪，跳过");
        return;
    };

    // 3. 创建编码器并执行渲染通道
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("video_export_render_encoder"),
    });

    prepare_renderers(renderers, &params, note_events_rx, device, queue);

    let hires_visible = update_hires_viewport(
        hires_renderer,
        hires_meta,
        hires_config,
        &params,
        device,
        queue,
    );
    let hires_visible_coords: Vec<crate::TileCoord> =
        hires_visible.iter().map(|(c, _)| *c).collect();

    execute_render_pass(
        &mut encoder,
        device,
        current_texture,
        depth_texture_view,
        &params,
        &mut renderers.grid,
        &mut renderers.note,
        &mut renderers.ruler,
        &mut renderers.arrangement,
        queue,
        &mut renderers.cc_bar,
        hires_renderer,
        &hires_visible_coords,
    );

    // 4. copy 离屏纹理到 staging buffer + submit + map_async
    pipeline.ensure_size(width, height);
    pipeline.copy_and_submit(encoder, texture, queue);

    // 5. 阻塞读回 BGRA 帧数据
    let frame_data = pipeline.wait_read();

    if frame_data.is_empty() {
        tracing::warn!("视频帧读回超时，发送空帧（可能丢帧）");
    }

    // 6. 发送给 Runner 线程写入 FFmpeg
    if tx.0.send(frame_data).is_err() {
        tracing::warn!("视频帧发送失败：Runner 通道已关闭");
    }
}
