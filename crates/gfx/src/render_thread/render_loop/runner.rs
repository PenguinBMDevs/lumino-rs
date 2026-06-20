use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::SwappableBuffer;
use crate::{KeyMode, OnionSkinRenderer};

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::execute_render_pass;
use super::render_pass::update_stats;
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
    note_events_rx: std::sync::mpsc::Receiver<crate::NoteEvent>,
    note_instances_buffer: Arc<SwappableBuffer<crate::NoteInstance>>,
    onion_progress: Arc<Mutex<Vec<(String, f32)>>>,
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut grid_renderer = crate::GridRenderer::new(&device, texture_format);
    let mut note_renderer = crate::NoteRenderer::new(&device, &queue, texture_format);
    let mut ruler_renderer = crate::RulerRenderer::new(&device, texture_format);
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

    // 洋葱皮渲染器状态：lazy 创建于首次 GenerateOnionSkin 命令
    let mut onion_skin_renderer: Option<OnionSkinRenderer> = None;
    let mut onion_key_mode: Option<KeyMode> = None;
    let mut deferred: Vec<ControlCommand> = Vec::new();

    while running.load(Ordering::Relaxed) {
        // 处理命令：先 drain 积压，然后如果没有 RenderCommand 则阻塞等待
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        let has_params = process_commands(
            &command_receiver,
            &mut latest_params,
            &mut should_shutdown,
            &mut deferred,
        );

        // 处理延迟的洋葱皮控制命令（需要 device/queue 上下文）
        for cmd in deferred.drain(..) {
            handle_onion_control(
                &mut onion_skin_renderer,
                &mut onion_key_mode,
                &device,
                &queue,
                texture_format,
                cmd,
                &onion_progress,
            );
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

                // 洋葱皮：进度轮询 + 贴图上传 + uniform 更新（在 render pass 之前）
                if let Some(ref mut onion) = onion_skin_renderer {
                    if let Some(progress) = onion.poll_progress() {
                        push_onion_progress(
                            &onion_progress,
                            "正在生成洋葱皮概览贴图…",
                            progress.percent() / 100.0,
                        );
                    }
                    if let Some(vp) = params.onion_skin_viewport {
                        if onion.check_and_upload(&device, &queue) {
                            push_onion_progress(&onion_progress, "洋葱皮概览贴图生成完成", 1.0);
                        }
                        onion.update_uniform(&queue, vp);
                    }
                }

                // 执行渲染通道
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
                    &mut cc_bar_renderer,
                    &onion_skin_renderer,
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

/// 处理洋葱皮控制命令（在渲染线程主循环中调用，拥有 device/queue 上下文）
#[allow(clippy::too_many_arguments)]
fn handle_onion_control(
    onion: &mut Option<OnionSkinRenderer>,
    onion_key_mode: &mut Option<KeyMode>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    cmd: ControlCommand,
    progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    match cmd {
        ControlCommand::GenerateOnionSkin {
            notes,
            duration_ms,
            key_mode,
        } => {
            // key_mode 变化时重建渲染器（贴图高度不同）
            if *onion_key_mode != Some(key_mode) {
                *onion = None; // Drop → dispose 旧渲染器
                let mut renderer = OnionSkinRenderer::new(device, queue, key_mode);
                renderer.update_render_format(device, texture_format);
                *onion = Some(renderer);
                *onion_key_mode = Some(key_mode);
            }
            if let Some(renderer) = onion.as_mut() {
                // tempo_table=None：以 tick 作为时间单位，与钢琴卷帘的 tick-线性映射对齐
                renderer.generate(device, queue, notes, duration_ms, None);
                push_onion_progress(progress, "正在生成洋葱皮概览贴图…", 0.0);
            }
        }
        ControlCommand::DisposeOnionSkin if onion.is_some() => {
            *onion = None; // Drop → dispose，取消后台生成线程
            *onion_key_mode = None;
            // 关闭可能仍开启的进度窗口
            push_onion_progress(progress, "洋葱皮资源已释放", 1.0);
        }
        // Resize / Shutdown 已在 process_commands 中处理，不会到达此处
        _ => {}
    }
}

/// 向共享进度缓冲推送一条进度（渲染线程 → UI 线程）
fn push_onion_progress(progress: &Arc<Mutex<Vec<(String, f32)>>>, msg: &str, value: f32) {
    if let Ok(mut buf) = progress.lock() {
        buf.push((msg.to_string(), value.clamp(0.0, 1.0)));
    }
}
