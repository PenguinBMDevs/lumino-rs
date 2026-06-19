use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::SwappableBuffer;

use super::super::commands::RenderCommand;
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::execute_render_pass;
use super::render_pass::update_stats;
use super::textures::ensure_textures;
use crate::OnionViewportUniform;

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
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut grid_renderer = crate::GridRenderer::new(&device, texture_format);
    let mut note_renderer = crate::NoteRenderer::new(&device, &queue, texture_format);
    let mut ruler_renderer = crate::RulerRenderer::new(&device, texture_format);
    let mut onion_renderer = crate::OnionRenderer::new(&device, &queue, texture_format);
    let mut arrangement_renderer = crate::ArrangementRenderer::new(&device, texture_format);
    let mut cc_bar_renderer = crate::CcBarRenderer::new(&device, texture_format);

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);
    let mut last_note_version: u64 = 0;

    while running.load(Ordering::Relaxed) {
        // 处理命令：先 drain 积压，然后如果没有 RenderCommand 则阻塞等待
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        let has_params =
            process_commands(&command_receiver, &mut latest_params, &mut should_shutdown);

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
            let mut tex_resources = super::textures::OffscreenTextureResources {
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

                note_renderer.upload_instances(notes, &device, &queue);
            }

            // ── 洋葱皮：全量 GPU 上传 + dirty tracking ──
            // 只在上游数据版本/颜色变化时才重传，静止帧跳过 upload（零分配）
            if params.onion_enabled {
                if let Some(note_list) = &params.onion_note_list {
                    let list_version = note_list.version();
                    let track_colors = params.onion_track_colors.as_deref().unwrap_or(&[]);
                    let color_hash = track_colors
                        .iter()
                        .fold(0u64, |acc, &c| acc.wrapping_mul(31).wrapping_add(c as u64));
                    onion_renderer.upload_notes(
                        note_list.as_slice(),
                        list_version,
                        track_colors,
                        color_hash,
                        &device,
                        &queue,
                    );
                } else {
                    onion_renderer.upload_notes(&[], 0, &[], 0, &device, &queue);
                }
            } else {
                onion_renderer.upload_notes(&[], 0, &[], 0, &device, &queue);
            }

            if let (Some(_texture), Some(_depth_view)) = (&current_texture, &depth_texture_view) {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("offscreen_render_encoder"),
                });

                // 准备渲染器
                prepare_renderers(
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut ruler_renderer,
                    &mut arrangement_renderer,
                    &mut cc_bar_renderer,
                    params,
                    &note_events_rx,
                    &device,
                    &queue,
                );

                // 准备洋葱皮视口 uniform（参考 Wasabi push constants 方式）
                let viewport = OnionViewportUniform {
                    tick_start: params.scroll.0 / params.zoom.0,
                    tick_end: (params.scroll.0 + params.canvas_size.0 - params.keyboard_width)
                        / params.zoom.0,
                    pitch_min: 0.0,
                    pitch_max: params.max_key_index,
                    note_count: onion_renderer.note_count(),
                    current_track: params.onion_current_track as u32,
                    keyboard_width: params.keyboard_width,
                    ruler_height: params.ruler_height,
                    canvas_width: params.logical_size.0,
                    canvas_height: params.logical_size.1,
                    canvas_offset_x: params.canvas_offset.0,
                    canvas_offset_y: params.canvas_offset.1,
                    scroll_x: params.scroll.0,
                    scroll_y: params.scroll.1,
                    zoom_x: params.zoom.0,
                    zoom_y: params.zoom.1,
                    max_key_index: params.max_key_index,
                };

                onion_renderer.prepare_viewport(&viewport, &queue);

                // 执行渲染通道（含洋葱皮背景和主音符）
                execute_render_pass(
                    &mut encoder,
                    &device,
                    &current_texture,
                    &depth_texture_view,
                    params,
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut ruler_renderer,
                    &mut arrangement_renderer,
                    &queue,
                    &mut onion_renderer,
                    &mut cc_bar_renderer,
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
