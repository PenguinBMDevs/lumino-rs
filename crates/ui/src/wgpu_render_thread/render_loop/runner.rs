use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use iced_wgpu::wgpu;
use lumino_gfx::SwappableBuffer;

use super::super::commands::RenderCommand;
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::{execute_render_pass, update_stats};
use super::textures::ensure_textures;

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
    onion_skin_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut grid_renderer = lumino_gfx::GridRenderer::new(&device, texture_format);
    let mut note_renderer = lumino_gfx::NoteRenderer::new(&device, &queue, texture_format);
    let mut keyboard_renderer = lumino_gfx::KeyboardRenderer::new(&device, texture_format);
    let mut ruler_renderer = lumino_gfx::RulerRenderer::new(&device, texture_format);

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);
    let mut last_note_version: u64 = 0;
    let mut last_onion_version: u64 = 0;
    // 可重用合并缓冲区，避免每帧分配
    let mut merged_instances: Vec<lumino_gfx::NoteInstance> = Vec::new();

    while running.load(Ordering::Relaxed) {
        // 处理所有待处理的命令
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        process_commands(&command_receiver, &mut latest_params, &mut should_shutdown);

        if should_shutdown {
            break;
        }

        // 执行渲染（离屏纹理）
        if let Some(ref params) = latest_params {
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

            // 分别检测主音符和洋葱皮版本号，合并后上传（任一变化都触发上传）
            let note_version = note_instances_buffer.version();
            let onion_version = onion_skin_instances_buffer.version();
            if note_version != last_note_version || onion_version != last_onion_version {
                last_note_version = note_version;
                last_onion_version = onion_version;

                puffin::profile_scope!("upload_note_instances_from_buffer");
                let notes = unsafe { note_instances_buffer.read_buffer() };
                let onion = unsafe { onion_skin_instances_buffer.read_buffer() };

                // 重用到合并缓冲区，避免每帧分配
                merged_instances.clear();
                merged_instances.reserve(notes.len() + onion.len());
                merged_instances.extend_from_slice(notes);
                merged_instances.extend_from_slice(onion);

                note_renderer.upload_instances(&merged_instances, &device, &queue);
            }

            if let (Some(texture), Some(_depth_view)) = (&current_texture, &depth_texture_view) {
                let _view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("offscreen_render_encoder"),
                });

                // 准备渲染器
                prepare_renderers(
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut keyboard_renderer,
                    &mut ruler_renderer,
                    params,
                    &note_events_rx,
                    &device,
                    &queue,
                );

                // 执行渲染通道
                execute_render_pass(
                    &mut encoder,
                    &current_texture,
                    &depth_texture_view,
                    params,
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut keyboard_renderer,
                    &mut ruler_renderer,
                    &queue,
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
        } else {
            // 没有新的渲染参数，短暂休眠避免 CPU 空转
            thread::sleep(Duration::from_micros(100));
        }
    }

    tracing::info!("Render thread stopped");
}
