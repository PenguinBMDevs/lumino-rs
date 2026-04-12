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

use lumino_gfx::{GridLineInstance, KeyInstance, NoteInstance, RulerTickInstance};

/// 渲染参数 - 从 UI 线程传递到 WGPU 线程
#[derive(Debug, Clone)]
pub struct RenderParams {
    /// 视口大小
    pub viewport_size: (u32, u32),
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
    /// 网格线实例
    pub grid_instances: Vec<GridLineInstance>,
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
            scroll: (0.0, 0.0),
            zoom: (0.1, 20.0),
            keyboard_width: 60.0,
            ruler_height: 30.0,
            background_color: [0.1, 0.1, 0.1, 1.0],
            grid_instances: Vec::new(),
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
}

impl WgpuRenderThread {
    /// 创建并启动渲染线程
    ///
    /// 注意：由于 wgpu Surface 限制，窗口必须在渲染线程中创建
    /// 或者使用已有的 Surface（如果在外部创建）
    pub fn spawn(
        _window: Arc<winit::window::Window>,
        _note_buffer: Arc<lumino_gfx::SwappableBuffer<NoteInstance>>,
    ) -> anyhow::Result<Self> {
        tracing::info!("WgpuRenderThread::spawn - Starting render thread");

        let stats = Arc::new(Mutex::new(RenderStats::default()));
        let running = Arc::new(AtomicBool::new(true));
        let (command_sender, command_receiver) = std::sync::mpsc::channel::<RenderCommand>();

        let stats_clone = Arc::clone(&stats);
        let running_clone = Arc::clone(&running);

        // 启动渲染线程
        let thread_handle = thread::spawn(move || {
            tracing::info!("Render thread started");

            // 渲染循环
            let mut frame_count = 0u64;
            let mut fps_update_time = Instant::now();

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
                            // 处理窗口大小变化
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

                // 执行渲染（占位实现）
                if let Some(params) = latest_params {
                    let frame_start = Instant::now();

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
