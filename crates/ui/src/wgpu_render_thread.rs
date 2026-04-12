//! WGPU 渲染线程 - 真正分离的渲染架构
//!
//! 架构说明：
//! - UI 线程（主线程）：处理事件、更新状态、生成渲染数据
//! - WGPU 渲染线程：管理 GPU 资源、执行渲染、Present 到 Surface
//!
//! 线程通信：
//! - 使用 mpsc 通道传递渲染命令
//! - 使用 SwappableBuffer 零拷贝共享音符数据
//! - 使用 Arc<Atomic*> 共享状态

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use iced_wgpu::wgpu;
use lumino_gfx::{GridLineInstance, KeyInstance, NoteInstance, RulerTickInstance};

/// 渲染参数 - 从 UI 线程传递到 WGPU 线程
#[derive(Debug, Clone)]
pub struct RenderParams {
    /// 物理视口大小
    pub viewport_size: (u32, u32),
    /// 逻辑视口大小
    pub logical_size: (f32, f32),
    /// 缩放因子
    pub scale_factor: f32,
    /// 滚动位置 (x, y)
    pub scroll: (f32, f32),
    /// 缩放 (x, y)
    pub zoom: (f32, f32),
    /// 键盘宽度
    pub keyboard_width: f32,
    /// 标尺高度
    pub ruler_height: f32,
    /// 背景颜色
    pub background_color: [f64; 4],
    /// 网格相关颜色 (用于 Shader)
    pub color_bg: [f32; 4],
    pub color_bg_black_key: [f32; 4],
    pub color_bar: [f32; 4],
    pub color_beat: [f32; 4],
    pub color_grid: [f32; 4],
    pub color_key_line: [f32; 4],
    /// 网格线实例
    pub grid_instances: Vec<GridLineInstance>,
    /// 音符实例
    pub note_instances: Vec<NoteInstance>,
    /// 标尺刻度实例
    pub ruler_instances: Vec<RulerTickInstance>,
    /// 琴键实例
    pub keyboard_instances: Vec<KeyInstance>,
    /// 每小节 tick 数
    pub ticks_per_measure: u32,
    /// 每拍 tick 数
    pub ticks_per_beat: u32,
    /// 是否需要重新生成网格
    pub regenerate_grid: bool,
    /// Canvas 偏移
    pub canvas_offset: (f32, f32),
    /// Canvas 大小
    pub canvas_size: (f32, f32),
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            viewport_size: (800, 600),
            logical_size: (800.0, 600.0),
            scale_factor: 1.0,
            scroll: (0.0, 0.0),
            zoom: (0.1, 20.0),
            keyboard_width: 60.0,
            ruler_height: 30.0,
            background_color: [0.1, 0.1, 0.1, 1.0],
            color_bg: [0.1, 0.1, 0.1, 1.0],
            color_bg_black_key: [0.07, 0.07, 0.07, 1.0],
            color_bar: [0.3, 0.3, 0.3, 1.0],
            color_beat: [0.2, 0.2, 0.2, 1.0],
            color_grid: [0.15, 0.15, 0.15, 1.0],
            color_key_line: [0.15, 0.15, 0.15, 1.0],
            grid_instances: Vec::new(),
            note_instances: Vec::new(),
            ruler_instances: Vec::new(),
            keyboard_instances: Vec::new(),
            ticks_per_measure: 1920,
            ticks_per_beat: 480,
            regenerate_grid: true,
            canvas_offset: (0.0, 0.0),
            canvas_size: (800.0, 600.0),
        }
    }
}

/// 控制命令
#[derive(Debug)]
pub enum ControlCommand {
    /// 调整窗口大小
    Resize { width: u32, height: u32 },
    /// 停止渲染线程
    Shutdown,
}

/// 渲染命令（UI 线程 -> 渲染线程）
#[derive(Debug)]
pub enum RenderCommand {
    /// 渲染一帧
    Render(RenderParams),
    /// 控制命令
    Control(ControlCommand),
}

/// 渲染统计
#[derive(Debug, Default, Clone)]
pub struct RenderStats {
    /// 总帧数
    pub frame_count: u64,
    /// 上一帧耗时 (ms)
    pub last_frame_time_ms: f64,
    /// 平均 FPS
    pub average_fps: f64,
    /// 丢弃的帧数
    pub dropped_frames: u64,
    /// 渲染的音符数量
    pub note_count: usize,
    /// 渲染的网格线数量
    pub grid_line_count: usize,
    /// 渲染的琴键数量
    pub key_count: usize,
    /// 渲染的标尺刻度数量
    pub ruler_tick_count: usize,
}

/// WGPU 渲染线程
///
/// 真正独立的渲染线程，管理所有 GPU 资源和渲染操作
pub struct WgpuRenderThread {
    /// 渲染统计
    pub stats: Arc<Mutex<RenderStats>>,
    /// 运行状态
    running: Arc<AtomicBool>,
    /// 渲染命令发送端
    command_sender: Option<std::sync::mpsc::Sender<RenderCommand>>,
    /// 线程句柄
    thread_handle: Option<JoinHandle<()>>,
    /// 渲染完成的离屏纹理，供主线程读取
    pub latest_texture: Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
}

impl WgpuRenderThread {
    /// 创建并启动渲染线程
    ///
    /// 采用离屏纹理架构：
    /// WGPU 渲染线程在后台将所有内容渲染到离屏纹理中，然后主线程将该纹理复制到 Surface。
    pub fn spawn(
        device: wgpu::Device,
        queue: wgpu::Queue,
        texture_format: wgpu::TextureFormat,
        note_events_rx: std::sync::mpsc::Receiver<lumino_gfx::NoteEvent>,
    ) -> anyhow::Result<Self> {
        tracing::info!("WgpuRenderThread::spawn - Starting render thread with offscreen texture");

        let stats = Arc::new(Mutex::new(RenderStats::default()));
        let running = Arc::new(AtomicBool::new(true));
        let (command_sender, command_receiver) = std::sync::mpsc::channel::<RenderCommand>();
        let latest_texture: Arc<Mutex<Option<Arc<wgpu::Texture>>>> = Arc::new(Mutex::new(None));

        let stats_clone = Arc::clone(&stats);
        let running_clone = Arc::clone(&running);
        let latest_texture_clone = Arc::clone(&latest_texture);

        // 启动渲染线程
        let thread_handle = thread::spawn(move || {
            tracing::info!("Render thread started");

            // 初始化渲染器
            let mut grid_renderer = lumino_gfx::GridRenderer::new(&device, texture_format);
            let mut note_renderer = lumino_gfx::NoteRenderer::new(&device, &queue, texture_format);
            let mut keyboard_renderer = lumino_gfx::KeyboardRenderer::new(&device, texture_format);
            let mut ruler_renderer = lumino_gfx::RulerRenderer::new(&device, texture_format);

            // 渲染循环
            let mut frame_count = 0u64;
            let mut fps_update_time = Instant::now();
            let mut current_texture: Option<Arc<wgpu::Texture>> = None;
            let mut depth_texture: Option<wgpu::Texture> = None;
            let mut depth_texture_view: Option<wgpu::TextureView> = None;
            let mut current_size = (0, 0);

            while running_clone.load(Ordering::Relaxed) {
                // 处理所有待处理的命令
                let mut latest_params: Option<RenderParams> = None;
                let mut should_shutdown = false;

                while let Ok(cmd) = command_receiver.try_recv() {
                    match cmd {
                        RenderCommand::Render(params) => {
                            latest_params = Some(params);
                        }
                        RenderCommand::Control(ControlCommand::Resize { width, height }) => {
                            tracing::debug!("Render thread: resize to {}x{}", width, height);
                        }
                        RenderCommand::Control(ControlCommand::Shutdown) => {
                            should_shutdown = true;
                            break;
                        }
                    }
                }

                if should_shutdown {
                    break;
                }

                // 执行渲染（离屏纹理）
                if let Some(params) = latest_params {
                    puffin::profile_scope!("wgpu_render_thread_frame");
                    let frame_start = Instant::now();

                    let width = params.viewport_size.0.max(1);
                    let height = params.viewport_size.1.max(1);

                    // 如果尺寸改变，重新创建离屏纹理
                    if current_size != (width, height)
                        || current_texture.is_none()
                        || depth_texture.is_none()
                    {
                        let texture = device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("offscreen_render_texture"),
                            size: wgpu::Extent3d {
                                width,
                                height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: texture_format,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::COPY_SRC,
                            view_formats: &[],
                        });
                        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("depth_texture"),
                            size: wgpu::Extent3d {
                                width,
                                height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Depth32Float,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            view_formats: &[],
                        });
                        depth_texture_view =
                            Some(depth_tex.create_view(&wgpu::TextureViewDescriptor::default()));
                        depth_texture = Some(depth_tex);
                        current_texture = Some(Arc::new(texture));
                        current_size = (width, height);

                        // 将新纹理共享给主线程
                        if let Ok(mut lock) = latest_texture_clone.lock() {
                            *lock = current_texture.clone();
                        }
                    }

                    if let (Some(texture), Some(depth_view)) =
                        (&current_texture, &depth_texture_view)
                    {
                        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("offscreen_render_encoder"),
                            });

                        let clear_color = wgpu::Color {
                            r: params.background_color[0],
                            g: params.background_color[1],
                            b: params.background_color[2],
                            a: params.background_color[3],
                        };

                        {
                            puffin::profile_scope!("prepare_renderers");
                            // 准备渲染实例
                            if !params.grid_instances.is_empty() || true {
                                grid_renderer.prepare(
                                    &[], // 不再传递 CPU 实例
                                    &device,
                                    &queue,
                                    params.canvas_size,
                                    params.scroll.0,
                                    params.scroll.1,
                                    params.zoom.0,
                                    params.zoom.1,
                                    params.keyboard_width,
                                    params.ruler_height,
                                    params.color_bg,
                                    params.color_bg_black_key,
                                    params.color_bar,
                                    params.color_beat,
                                    params.color_grid,
                                    params.color_key_line,
                                );
                            }

                            note_renderer.process_events(&note_events_rx, &device, &queue);

                            if !params.keyboard_instances.is_empty() {
                                keyboard_renderer.prepare(
                                    &device,
                                    &queue,
                                    params.logical_size,
                                    params.keyboard_width,
                                    params.ruler_height,
                                    params.scroll.1,
                                    params.zoom.1,
                                    128,
                                );
                            }
                            if !params.ruler_instances.is_empty() {
                                ruler_renderer.prepare(
                                    &device,
                                    &queue,
                                    params.logical_size,
                                    params.keyboard_width,
                                    params.ruler_height,
                                    params.scroll.0,
                                    params.zoom.0,
                                    params.ticks_per_measure,
                                    params.ticks_per_beat,
                                );
                            }
                        }

                        // 准备相机参数
                        let camera = lumino_gfx::CameraUniform::new(lumino_gfx::CameraParams {
                            scroll: [params.scroll.0, params.scroll.1],
                            zoom: [params.zoom.0, params.zoom.1],
                            viewport: [params.logical_size.0, params.logical_size.1],
                            offset: [params.canvas_offset.0, params.canvas_offset.1],
                            keyboard_width: params.keyboard_width,
                            ruler_height: params.ruler_height,
                            max_key_index: 127.0, // TODO: 从 params 获取
                        });

                        note_renderer.prepare_pass(&mut encoder, camera, &queue);

                        {
                            puffin::profile_scope!("render_pass");
                            let mut render_pass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("offscreen_render_pass"),
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Clear(clear_color),
                                            store: wgpu::StoreOp::Store,
                                        },
                                        depth_slice: None,
                                    })],
                                    depth_stencil_attachment: Some(
                                        wgpu::RenderPassDepthStencilAttachment {
                                            view: depth_view,
                                            depth_ops: Some(wgpu::Operations {
                                                load: wgpu::LoadOp::Clear(1.0),
                                                store: wgpu::StoreOp::Store,
                                            }),
                                            stencil_ops: None,
                                        },
                                    ),
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });

                            // 计算裁剪区域
                            let scale = params.scale_factor;
                            let scissor_x = ((params.canvas_offset.0 * scale) as u32).min(width);
                            let scissor_y = ((params.canvas_offset.1 * scale) as u32).min(height);
                            let scissor_width = ((params.canvas_size.0 * scale) as u32)
                                .min(width.saturating_sub(scissor_x));
                            let scissor_height = ((params.canvas_size.1 * scale) as u32)
                                .min(height.saturating_sub(scissor_y));

                            // 绘制背景网格
                            {
                                render_pass.set_scissor_rect(
                                    scissor_x,
                                    scissor_y,
                                    scissor_width,
                                    scissor_height,
                                );
                                grid_renderer.draw(&mut render_pass, 1); // instance_count=1 for quad
                            }

                            // 绘制音符
                            render_pass.set_scissor_rect(
                                scissor_x,
                                scissor_y,
                                scissor_width,
                                scissor_height,
                            );
                            note_renderer.draw(
                                &mut render_pass,
                                true, // or dynamically check if instances > 0? Actually, true is fine, it will draw 0 instances if empty
                                Some((scissor_x, scissor_y, scissor_width, scissor_height)),
                            );

                            // 绘制键盘（不受画布裁剪限制）
                            if !params.keyboard_instances.is_empty() {
                                render_pass.set_scissor_rect(0, 0, width, height);
                                keyboard_renderer
                                    .draw(&mut render_pass, params.keyboard_instances.len() as u32);
                            }

                            // 绘制标尺（不受画布裁剪限制）
                            if !params.ruler_instances.is_empty() {
                                render_pass.set_scissor_rect(0, 0, width, height);
                                ruler_renderer
                                    .draw(&mut render_pass, params.ruler_instances.len() as u32);
                            }
                        }

                        // 提交渲染指令
                        {
                            puffin::profile_scope!("submit_queue");
                            queue.submit(std::iter::once(encoder.finish()));
                        }
                    }

                    // 更新统计
                    let frame_time = frame_start.elapsed();
                    frame_count += 1;

                    if let Ok(mut stats) = stats_clone.lock() {
                        stats.frame_count = frame_count;
                        stats.last_frame_time_ms = frame_time.as_secs_f64() * 1000.0;
                        stats.grid_line_count = params.grid_instances.len();
                        stats.key_count = params.keyboard_instances.len();
                        stats.ruler_tick_count = params.ruler_instances.len();
                    }

                    // 更新 FPS
                    if fps_update_time.elapsed().as_secs() >= 1 {
                        if let Ok(mut stats) = stats_clone.lock() {
                            stats.average_fps =
                                frame_count as f64 / fps_update_time.elapsed().as_secs_f64();
                        }
                        frame_count = 0;
                        fps_update_time = Instant::now();
                    }
                } else {
                    // 没有新的渲染参数，短暂休眠避免 CPU 空转
                    thread::sleep(Duration::from_micros(100));
                }
            }

            tracing::info!("Render thread stopped");
        });

        Ok(Self {
            stats,
            running,
            command_sender: Some(command_sender),
            thread_handle: Some(thread_handle),
            latest_texture,
        })
    }

    /// 发送渲染参数
    pub fn send_params(&self, params: RenderParams) {
        if let Some(ref sender) = self.command_sender {
            // 使用非阻塞发送，如果通道满则丢弃旧帧
            // 注意：std::sync::mpsc 没有 try_send，我们使用 send 并设置较小的通道容量
            match sender.send(RenderCommand::Render(params)) {
                Ok(_) => {}
                Err(_) => {
                    // 通道关闭或满，丢弃这一帧
                    if let Ok(mut stats) = self.stats.lock() {
                        stats.dropped_frames += 1;
                    }
                }
            }
        }
    }

    /// 发送控制命令
    pub fn send_control(&self, cmd: ControlCommand) {
        if let Some(ref sender) = self.command_sender {
            if let Err(e) = sender.send(RenderCommand::Control(cmd)) {
                tracing::warn!("Failed to send control command: {}", e);
            }
        }
    }

    /// 获取渲染统计
    pub fn stats(&self) -> RenderStats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 关闭渲染线程
    pub fn shutdown(mut self) {
        self.running.store(false, Ordering::Relaxed);

        // 发送关闭命令
        if let Some(ref sender) = self.command_sender {
            let _ = sender.send(RenderCommand::Control(ControlCommand::Shutdown));
        }

        // 等待线程结束
        if let Some(handle) = self.thread_handle.take() {
            if let Err(e) = handle.join() {
                tracing::error!("Render thread panicked: {:?}", e);
            }
        }

        tracing::info!("WgpuRenderThread::shutdown - Render thread stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_params_default() {
        let params = RenderParams::default();
        assert_eq!(params.viewport_size, (800, 600));
        assert_eq!(params.keyboard_width, 60.0);
        assert_eq!(params.ruler_height, 30.0);
    }

    #[test]
    fn test_render_stats_default() {
        let stats = RenderStats::default();
        assert_eq!(stats.frame_count, 0);
        assert_eq!(stats.dropped_frames, 0);
    }

    #[test]
    fn test_control_command_debug() {
        let cmd = ControlCommand::Resize {
            width: 1920,
            height: 1080,
        };
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("Resize"));
    }

    #[test]
    fn test_render_command_debug() {
        let params = RenderParams::default();
        let cmd = RenderCommand::Render(params);
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("Render"));
    }
}
