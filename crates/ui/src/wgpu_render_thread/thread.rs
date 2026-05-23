use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

use iced_wgpu::wgpu;

use super::commands::{ControlCommand, RenderCommand};
use super::params::RenderParams;
use super::render_loop::run_render_thread;
use super::stats::RenderStats;
use lumino_gfx::SwappableBuffer;

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
    /// 双缓冲音符实例数据（UI线程写入，渲染线程读取）
    pub note_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
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
        note_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
        onion_note_buffer: Option<Arc<SwappableBuffer<lumino_gfx::OnionNote>>>,
    ) -> anyhow::Result<Self> {
        tracing::info!("WgpuRenderThread::spawn - Starting render thread with offscreen texture");

        let stats = Arc::new(Mutex::new(RenderStats::default()));
        let running = Arc::new(AtomicBool::new(true));
        let (command_sender, command_receiver) = std::sync::mpsc::channel::<RenderCommand>();
        let latest_texture: Arc<Mutex<Option<Arc<wgpu::Texture>>>> = Arc::new(Mutex::new(None));

        let stats_clone = Arc::clone(&stats);
        let running_clone = Arc::clone(&running);
        let latest_texture_clone = Arc::clone(&latest_texture);
        let note_instances_buffer_clone = Arc::clone(&note_instances_buffer);
        let onion_note_buffer_clone = onion_note_buffer.clone();

        // 启动渲染线程
        let thread_handle = thread::spawn(move || {
            run_render_thread(
                device,
                queue,
                texture_format,
                running_clone,
                command_receiver,
                latest_texture_clone,
                stats_clone,
                note_events_rx,
                note_instances_buffer_clone,
                onion_note_buffer_clone,
            );
        });

        Ok(Self {
            stats,
            running,
            command_sender: Some(command_sender),
            thread_handle: Some(thread_handle),
            latest_texture,
            note_instances_buffer,
        })
    }

    /// 发送渲染参数
    pub fn send_params(&self, params: RenderParams) {
        if let Some(ref sender) = self.command_sender {
            // 使用非阻塞发送，如果通道满则丢弃旧帧
            match sender.send(RenderCommand::Render(Box::new(params))) {
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
        if let Some(ref sender) = self.command_sender
            && let Err(e) = sender.send(RenderCommand::Control(cmd))
        {
            tracing::warn!("Failed to send control command: {}", e);
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
        if let Some(handle) = self.thread_handle.take()
            && let Err(e) = handle.join()
        {
            tracing::error!("Render thread panicked: {:?}", e);
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
        let cmd = RenderCommand::Render(Box::new(params));
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("Render"));
    }
}
