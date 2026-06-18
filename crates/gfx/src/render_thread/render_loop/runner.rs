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
use crate::{
    CameraParams, CameraUniform, OnionCollectParams, OnionTrackColors,
    OnionTrackMask, OnionViewportUniform,
};

/// 渲染线程持有的洋葱皮游标状态
struct OnionCursorState {
    cursors: Box<[usize; 256]>,
    last_tick_start: f32,
    last_bucket_version: u64,
}

impl OnionCursorState {
    fn new() -> Self {
        Self {
            cursors: Box::new([0; 256]),
            last_tick_start: 0.0,
            last_bucket_version: u64::MAX,
        }
    }

    fn reset(&mut self, bucket_version: u64, tick_start: f32) {
        self.cursors.fill(0);
        self.last_bucket_version = bucket_version;
        self.last_tick_start = tick_start;
    }
}

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
    let mut last_onion_mask: Option<OnionTrackMask> = None;
    let mut last_onion_colors: Option<OnionTrackColors> = None;
    // 洋葱皮采集游标（渲染线程本地，跨帧持久）
    let mut onion_cursor = OnionCursorState::new();
    // 临时 Vec 复用，避免每帧分配
    let mut onion_notes_buf: Vec<crate::OnionNote> = Vec::with_capacity(256 * 1024);

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

            // M3: 上传轨道掩码（仅变化时）
            if let Some(mask) = &params.onion_track_mask
                && last_onion_mask.as_ref() != Some(mask)
            {
                onion_renderer.upload_track_mask(mask, &queue);
                last_onion_mask = Some(*mask);
            }

            // M3: 上传轨道颜色表（仅变化时）
            if let Some(colors) = &params.onion_track_colors
                && last_onion_colors.as_ref() != Some(colors)
            {
                onion_renderer.upload_track_colors(colors, &queue);
                last_onion_colors = Some(*colors);
            }

            // ── 方案 C：渲染线程直接采集洋葱皮 ──
            if params.onion_enabled {
                if let Some(bucket) = &params.onion_bucket {
                    // 数据变化时重置游标
                    if params.onion_bucket_version != onion_cursor.last_bucket_version {
                        onion_cursor.reset(
                            params.onion_bucket_version,
                            params.onion_overscan_ticks,
                        );
                    }

                    // 计算可见 tick 范围（与 notes.rs 一致）
                    let note_area_width =
                        (params.canvas_size.0 - params.keyboard_width).max(0.0);
                    let visible_tick_start = (params.scroll.0 / params.zoom.0).max(0.0);
                    let visible_tick_end =
                        ((params.scroll.0 + note_area_width) / params.zoom.0)
                            .max(visible_tick_start);

                    // 右侧 overscan
                    let right_pad = params
                        .onion_overscan_ticks
                        .min((visible_tick_end - visible_tick_start) * 1.5);
                    let extended_end = (visible_tick_end + right_pad).max(0.0);

                    // 计算可见 key 范围（与 notes.rs 一致）
                    let note_area_height =
                        (params.canvas_size.1 - params.ruler_height).max(0.0);
                    let max_key = params.max_key_index;
                    let key_top = max_key - (params.scroll.1 / params.zoom.1);
                    let key_bottom =
                        max_key - ((params.scroll.1 + note_area_height) / params.zoom.1);
                    let visible_key_max = (key_top.ceil() as u16 + 1).min(255);
                    let visible_key_min =
                        (key_bottom.floor().max(0.0) as u16).saturating_sub(1);

                    // 采集可见音符
                    onion_notes_buf.clear();
                    let current_track = params.onion_current_track;
                    let track_filter = |track_idx: u16| track_idx != current_track;

                    bucket.collect_visible_with_cursor(
                        OnionCollectParams::new(
                            visible_tick_start,
                            extended_end,
                            visible_key_min,
                            visible_key_max,
                            onion_cursor.last_tick_start,
                        ),
                        &mut onion_cursor.cursors,
                        track_filter,
                        &mut onion_notes_buf,
                    );

                    // 上限保护
                    const MAX_VISIBLE_NOTES: usize = 3_000_000;
                    if onion_notes_buf.len() > MAX_VISIBLE_NOTES {
                        onion_notes_buf.truncate(MAX_VISIBLE_NOTES);
                    }

                    // 直接上传到 GPU
                    onion_renderer.upload_notes(&onion_notes_buf, &device, &queue);

                    onion_cursor.last_tick_start = visible_tick_start;
                } else {
                    // 没有 bucket 时清空
                    onion_renderer.upload_notes(&[], &device, &queue);
                }
            } else {
                // 洋葱皮关闭时清空
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

                // 计算可见 tick/pitch 范围用于视口裁剪
                let note_area_width =
                    (params.canvas_size.0 - params.keyboard_width).max(0.0);
                let note_area_height =
                    (params.canvas_size.1 - params.ruler_height).max(0.0);

                let visible_tick_start = (params.scroll.0 / params.zoom.0).max(0.0);
                let visible_tick_end =
                    ((params.scroll.0 + note_area_width) / params.zoom.0)
                        .max(visible_tick_start);
                let max_key = params.max_key_index;
                let key_top = max_key - (params.scroll.1 / params.zoom.1);
                let key_bottom =
                    max_key - ((params.scroll.1 + note_area_height) / params.zoom.1);
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
                    None,
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
