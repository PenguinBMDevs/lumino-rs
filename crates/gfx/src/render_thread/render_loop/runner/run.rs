use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::SwappableBuffer;

use super::super::super::commands::{ControlCommand, RenderCommand};
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

        // 处理延迟的高精度洋葱皮控制命令
        for cmd in deferred.drain(..) {
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
                    &mut renderers,
                    &queue,
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
