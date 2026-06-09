use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use iced_wgpu::wgpu;
use lumino_gfx::SwappableBuffer;

use super::super::commands::RenderCommand;
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::execute_render_pass;
use super::render_pass::update_stats;
use super::textures::ensure_textures;
use lumino_gfx::{CameraParams, CameraUniform, OnionViewportUniform};

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
    note_events_rx: std::sync::mpsc::Receiver<lumino_gfx::NoteEvent>,
    note_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    onion_note_buffer: Option<Arc<SwappableBuffer<lumino_gfx::OnionNote>>>,
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut grid_renderer = lumino_gfx::GridRenderer::new(&device, texture_format);
    let mut note_renderer = lumino_gfx::NoteRenderer::new(&device, &queue, texture_format);
    let mut ruler_renderer = lumino_gfx::RulerRenderer::new(&device, texture_format);
    let mut onion_renderer = lumino_gfx::OnionRenderer::new(&device, &queue, texture_format);
    let mut arrangement_renderer = lumino_gfx::ArrangementRenderer::new(&device, texture_format);
    let mut cc_bar_renderer = lumino_gfx::CcBarRenderer::new(&device, texture_format);

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);
    let mut last_note_version: u64 = 0;
    let mut last_onion_note_version: u64 = 0;

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
            ensure_textures(
                &device,
                texture_format,
                width,
                height,
                &mut current_size,
                &mut current_texture,
                &mut depth_texture,
                &mut depth_texture_view,
                &latest_texture_clone,
                params,
            );

            // 仅检测主音符版本号变化后上传
            let note_version = note_instances_buffer.version();
            if note_version != last_note_version {
                last_note_version = note_version;

                puffin::profile_scope!("upload_note_instances_from_buffer");
                let notes = unsafe { note_instances_buffer.read_buffer() };

                note_renderer.upload_instances(notes, &device, &queue);
            }

            // 检测洋葱皮音符数据变化，上传到 OnionRenderer
            // 先读 version，再读 buffer（消除 TOCTOU 竞态：原代码先 read_buffer 后 version，
            // 中间 worker swap 会导致上传旧数据。修正后至多一次额外上传，不产生错误渲染）
            let onion_notes_for_cull: Option<&[lumino_gfx::OnionNote]> =
                if let Some(buf) = &onion_note_buffer {
                    let ver = buf.version();
                    if ver != last_onion_note_version {
                        last_onion_note_version = ver;
                        let notes = unsafe { buf.read_buffer() };
                        if notes.is_empty() {
                            onion_renderer.upload_notes(&[], &device, &queue);
                        } else {
                            onion_renderer.upload_notes(notes, &device, &queue);
                        }
                    }
                    // 上传后重新读取用于 CPU cull range 二分查找
                    Some(unsafe { &**buf.read_buffer() })
                } else {
                    None
                };

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

                // 计算可见 tick/pitch 范围用于视口裁剪
                let visible_tick_start = (params.scroll.0 / params.zoom.0).max(0.0);
                let visible_tick_end = ((params.scroll.0 + params.logical_size.0) / params.zoom.0)
                    .max(visible_tick_start);
                let max_key = params.max_key_index;
                let key_top = max_key - (params.scroll.1 / params.zoom.1);
                let key_bottom =
                    max_key - ((params.scroll.1 + params.logical_size.1) / params.zoom.1);
                let visible_pitch_max = (key_top.ceil() as u16 + 1) as f32;
                let visible_pitch_min =
                    (key_bottom.floor().max(0.0) as u16).saturating_sub(1) as f32;

                let viewport = OnionViewportUniform {
                    tick_start: visible_tick_start,
                    tick_end: visible_tick_end,
                    pitch_min: visible_pitch_min,
                    pitch_max: visible_pitch_max,
                    note_count: 0,
                    indices_capacity: 65536,
                    visible_start: 0,
                    visible_end: 0,
                };

                onion_renderer.prepare_cull(
                    &mut encoder,
                    &viewport,
                    &camera,
                    &queue,
                    &device,
                    onion_notes_for_cull,
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
        // 注：process_commands 在没有 RenderCommand 时会阻塞在 recv()，
        // 因此此处不再需要 sleep。wgpu 线程在无工作时完全休眠，
        // 有工作时立即响应。
    }

    tracing::info!("Render thread stopped");
}
