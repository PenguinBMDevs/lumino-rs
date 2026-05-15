use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use std::time::{Duration, Instant};

use iced_wgpu::wgpu;
use lumino_gfx::SwappableBuffer;

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::{execute_render_pass, update_stats};
use super::textures::ensure_textures;

/// 渲染线程空闲时的等待超时
const IDLE_RECV_TIMEOUT: Duration = Duration::from_millis(16);

/// 运行渲染线程主循环
#[allow(clippy::too_many_arguments)]
pub fn run_render_thread(
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    running: Arc<AtomicBool>,
    command_receiver: std::sync::mpsc::Receiver<RenderCommand>,
    latest_texture_clone: Arc<RwLock<Option<Arc<wgpu::Texture>>>>,
    stats_clone: Arc<Mutex<RenderStats>>,
    note_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    pending_frames: Arc<AtomicU32>,
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

    while running.load(Ordering::Relaxed) {
        // 使用 recv_timeout 等待命令，避免 CPU 空转轮询
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        process_commands(&command_receiver, &mut latest_params, &mut should_shutdown);

        if should_shutdown {
            break;
        }

        // 如果没有待处理命令，阻塞等待（带超时）
        if latest_params.is_none() {
            match command_receiver.recv_timeout(IDLE_RECV_TIMEOUT) {
                Ok(RenderCommand::Render(params)) => {
                    latest_params = Some(*params);
                }
                Ok(RenderCommand::Control(ControlCommand::Shutdown)) => {
                    break;
                }
                Ok(RenderCommand::Control(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // 超时，继续循环检查 running 标志
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
            // 收到命令后，drain 剩余待处理命令，只保留最新
            while let Ok(cmd) = command_receiver.try_recv() {
                match cmd {
                    RenderCommand::Render(params) => {
                        latest_params = Some(*params);
                    }
                    RenderCommand::Control(ControlCommand::Shutdown) => {
                        should_shutdown = true;
                    }
                    RenderCommand::Control(_) => {}
                }
            }
            if should_shutdown {
                break;
            }
        }

        // 执行渲染（离屏纹理）
        if let Some(ref params) = latest_params {
            // 递减待处理帧计数（必须在渲染前，因为主线程依赖此计数做背压）
            pending_frames.fetch_sub(1, Ordering::Acquire);

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

            // 从双缓冲读取音符数据并上传到 GPU（仅在数据变化时）
            let current_version = note_instances_buffer.version();
            if current_version != last_note_version {
                last_note_version = current_version;
                let instances = unsafe { note_instances_buffer.read_buffer() };
                puffin::profile_scope!("upload_note_instances_from_buffer");
                note_renderer.upload_instances(instances, &device, &queue);
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
        }
    }

    tracing::info!("Render thread stopped");
}
