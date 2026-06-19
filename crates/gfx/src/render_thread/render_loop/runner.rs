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
use crate::{CameraParams, CameraUniform, OnionViewportUniform};

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

            // 计算可见 tick/pitch 范围（在 upload_bucket 和 prepare_cull 中都用到）
            let note_area_width = (params.canvas_size.0 - params.keyboard_width).max(0.0);
            let note_area_height = (params.canvas_size.1 - params.ruler_height).max(0.0);

            let visible_tick_start = (params.scroll.0 / params.zoom.0).max(0.0);
            let visible_tick_end =
                ((params.scroll.0 + note_area_width) / params.zoom.0).max(visible_tick_start);
            let max_key = params.max_key_index;
            let key_top = max_key - (params.scroll.1 / params.zoom.1);
            let key_bottom = max_key - ((params.scroll.1 + note_area_height) / params.zoom.1);
            let visible_pitch_max_overscan = (key_top.ceil() as u16 + 2) as f32; // +1 overscan
            let visible_pitch_min_overscan =
                (key_bottom.floor().max(0.0) as u16).saturating_sub(2) as f32; // -1 overscan

            // 上传 bucket 时只包含可见 key 范围的音符（避免 GPU storage buffer 溢出）
            let bucket_key_min = visible_pitch_min_overscan.max(0.0) as u8;
            let bucket_key_max = (visible_pitch_max_overscan.min(255.0) as u8).max(bucket_key_min);

            // ── 洋葱皮：GPU 常驻 bucket 模式 ──
            if params.onion_enabled {
                if let Some(bucket) = &params.onion_bucket {
                    let bucket_version = params.onion_bucket_version;
                    let track_colors = params.onion_track_colors.as_deref().unwrap_or(&[]);
                    let color_version = track_colors
                        .iter()
                        .fold(0u64, |acc, &c| acc.wrapping_mul(31).wrapping_add(c as u64));
                    onion_renderer.upload_bucket(
                        bucket,
                        bucket_version,
                        track_colors,
                        color_version,
                        bucket_key_min,
                        bucket_key_max,
                        visible_tick_start as u32,
                        visible_tick_end as u32,
                        params.zoom.0,
                        &device,
                        &queue,
                    );
                } else {
                    onion_renderer.upload_notes(&[], &device, &queue);
                }
            } else {
                onion_renderer.upload_notes(&[], &device, &queue);
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

                // 准备洋葱皮计算剔除（每帧执行，因为视口可能变化）
                let camera = CameraUniform::new(CameraParams {
                    scroll: [params.scroll.0, params.scroll.1],
                    zoom: [params.zoom.0, params.zoom.1],
                    viewport: [params.logical_size.0, params.logical_size.1],
                    offset: [params.canvas_offset.0, params.canvas_offset.1],
                    keyboard_width: params.keyboard_width,
                    ruler_height: params.ruler_height,
                    max_key_index: params.max_key_index,
                });

                let viewport = OnionViewportUniform {
                    tick_start: visible_tick_start,
                    tick_end: visible_tick_end,
                    pitch_min: visible_pitch_min_overscan,
                    pitch_max: visible_pitch_max_overscan,
                    note_count: onion_renderer.note_count() as u32,
                    indices_capacity: onion_renderer.indices_capacity() as u32,
                    current_track: params.onion_current_track as u32,
                    use_key_ranges: 0,
                    visible_start: 0,
                    visible_end: onion_renderer.note_count() as u32,
                };

                onion_renderer.prepare_cull(
                    &mut encoder,
                    &viewport,
                    &camera,
                    &queue,
                    &device,
                    params.onion_bucket.as_deref(),
                    params.onion_current_track,
                );

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
