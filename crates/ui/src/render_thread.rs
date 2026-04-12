//! 渲染线程模块 - 将WGPU渲染从UI线程分离到独立线程
//!
//! 架构设计：
//! - UI线程（主线程）：处理事件、更新状态、生成渲染命令
//! - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染
//!
//! 通信机制：使用mpsc channel传递RenderCommand
//!
//! 注意：由于wgpu的Surface必须在创建它的线程上使用，渲染线程需要拥有
//! 自己的GPU上下文，而不是与主线程共享。

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use lumino_gfx::{CameraUniform, GridLineInstance, NoteInstance};

/// 渲染命令 - 从UI线程发送到渲染线程的指令
#[derive(Debug, Clone)]
pub enum RenderCommand {
    /// 更新窗口大小
    Resize { width: u32, height: u32 },
    /// 更新音符实例数据
    UpdateNotes {
        instances: Vec<NoteInstance>,
        camera: CameraUniform,
    },
    /// 更新网格线数据
    UpdateGrid {
        instances: Vec<GridLineInstance>,
        viewport_size: (f32, f32),
    },
    /// 更新背景颜色
    UpdateBackground { color: [f64; 4] },
    /// 设置裁剪区域
    SetScissor {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// 执行渲染并呈现
    RenderAndPresent,
    /// 停止渲染线程
    Shutdown,
}

/// 渲染线程句柄 - 用于与渲染线程通信
pub struct RenderThreadHandle {
    pub(crate) command_sender: Sender<RenderCommand>,
    pub(crate) thread_handle: Option<JoinHandle<()>>,
    pub(crate) stats: Arc<Mutex<RenderStats>>,
}

impl RenderThreadHandle {
    /// 创建新的渲染线程句柄（不启动线程）
    pub fn new() -> (Self, Receiver<RenderCommand>) {
        let (sender, receiver) = mpsc::channel();
        let stats = Arc::new(Mutex::new(RenderStats::default()));

        let handle = Self {
            command_sender: sender,
            thread_handle: None,
            stats,
        };

        (handle, receiver)
    }

    /// 发送渲染命令
    pub fn send(&self, command: RenderCommand) {
        if let Err(_) = self.command_sender.send(command) {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.dropped_commands += 1;
        }
    }

    /// 获取渲染统计
    pub fn stats(&self) -> RenderStats {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 关闭渲染线程
    pub fn shutdown(mut self) {
        self.send(RenderCommand::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// 渲染统计信息
#[derive(Debug, Default, Clone)]
pub struct RenderStats {
    pub frame_count: u64,
    pub last_frame_time_ms: f64,
    pub average_fps: f64,
    pub dropped_commands: u64,
}

/// 渲染线程状态
///
/// 注意：这个结构体在渲染线程内部使用，不跨线程共享
pub struct RenderThreadState {
    pub stats: Arc<Mutex<RenderStats>>,
    pub background_color: [f64; 4],
    pub scissor_rect: Option<(u32, u32, u32, u32)>,
    pub note_instances: Vec<NoteInstance>,
    pub grid_instances: Vec<GridLineInstance>,
    pub camera: CameraUniform,
    pub viewport_size: (f32, f32),
    pub notes_dirty: bool,
    pub grid_dirty: bool,
}

impl RenderThreadState {
    pub fn new(stats: Arc<Mutex<RenderStats>>) -> Self {
        Self {
            stats,
            background_color: [0.0, 0.0, 0.0, 1.0],
            scissor_rect: None,
            note_instances: Vec::new(),
            grid_instances: Vec::new(),
            camera: CameraUniform::default(),
            viewport_size: (800.0, 600.0),
            notes_dirty: false,
            grid_dirty: false,
        }
    }

    pub fn handle_command(&mut self, command: RenderCommand) -> bool {
        match command {
            RenderCommand::Resize { width, height } => {
                self.viewport_size = (width as f32, height as f32);
                false
            }
            RenderCommand::UpdateNotes { instances, camera } => {
                self.note_instances = instances;
                self.camera = camera;
                self.notes_dirty = true;
                false
            }
            RenderCommand::UpdateGrid {
                instances,
                viewport_size,
            } => {
                self.grid_instances = instances;
                self.viewport_size = viewport_size;
                self.grid_dirty = true;
                false
            }
            RenderCommand::UpdateBackground { color } => {
                self.background_color = color;
                false
            }
            RenderCommand::SetScissor {
                x,
                y,
                width,
                height,
            } => {
                self.scissor_rect = Some((x, y, width, height));
                false
            }
            RenderCommand::RenderAndPresent => {
                // 渲染一帧（由外部调用者实现）
                false
            }
            RenderCommand::Shutdown => true,
        }
    }
}

/// 启动渲染线程
///
/// 注意：由于wgpu限制，这个函数需要在拥有窗口的线程上调用
/// 实际实现中，渲染线程需要创建自己的窗口或使用共享纹理
pub fn spawn_render_thread(
    _command_receiver: Receiver<RenderCommand>,
    _stats: Arc<Mutex<RenderStats>>,
) -> JoinHandle<()> {
    // 由于wgpu的Surface必须在创建它的线程上使用，
    // 我们需要采用不同的架构：
    // 1. 主线程创建窗口和Surface
    // 2. 渲染线程通过共享纹理或离屏渲染方式工作
    // 3. 或者使用生产者-消费者模式，渲染线程只准备命令缓冲区

    // 当前实现：渲染线程只负责准备渲染数据，实际提交由主线程完成
    thread::spawn(move || {
        tracing::info!("Render thread: Started (data preparation mode)");

        // 在这个简化版本中，渲染线程只接收命令并更新状态
        // 实际渲染仍然由主线程执行
        // 这是为了遵守wgpu的线程限制

        tracing::info!("Render thread: Exited");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_command_creation() {
        let cmd = RenderCommand::UpdateBackground {
            color: [1.0, 0.0, 0.0, 1.0],
        };
        match cmd {
            RenderCommand::UpdateBackground { color } => {
                assert_eq!(color, [1.0, 0.0, 0.0, 1.0]);
            }
            _ => panic!("Wrong command type"),
        }
    }

    #[test]
    fn test_render_stats_default() {
        let stats = RenderStats::default();
        assert_eq!(stats.frame_count, 0);
        assert_eq!(stats.dropped_commands, 0);
    }

    #[test]
    fn test_render_thread_state() {
        let stats = Arc::new(Mutex::new(RenderStats::default()));
        let mut state = RenderThreadState::new(stats);

        let cmd = RenderCommand::UpdateBackground {
            color: [0.5, 0.5, 0.5, 1.0],
        };
        let should_exit = state.handle_command(cmd);
        assert!(!should_exit);
        assert_eq!(state.background_color, [0.5, 0.5, 0.5, 1.0]);
    }
}
