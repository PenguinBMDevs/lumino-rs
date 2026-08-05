use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

use super::commands::{ControlCommand, RenderCommand};
use super::params::RenderParams;
use super::render_loop::run_render_thread;
use super::render_loop::runner::context::{RenderContext, RenderThreadChannels};
use super::stats::RenderStats;
use crate::SwappableBuffer;

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
    /// 音符事件发送端（UI 线程 → 渲染线程增量更新）
    ///
    /// 之前 bug：`enable_separate_render_thread()` 中 `let (_tx, rx) = channel()`
    /// `_tx` 立即 dropped 导致 `rx` 永远收到 Disconnected，`process_events()` 死信。
    /// 修复：sender 存储在此，通过 `send_note_event()` 暴露给 UI 线程。
    note_event_sender: Option<std::sync::mpsc::Sender<crate::NoteEvent>>,
    /// 洋葱皮流式上传发送端（UI 线程分块构建 → 渲染线程 streaming_append 到 GPU）
    ///
    /// 性能优化（6 亿音符 CPU 峰值 2026-08-05）：
    /// 旧方案通过 RenderParams.onion_skin_instances 全量传输（9.6 GB @ 6 亿音符），
    /// UI 线程构建临时峰值 14.4 GB（collected）+ 9.6 GB（instances Vec）。
    /// 新方案用 sync_channel(32) 分块流式传输，每块 ≤ 10 万实例（1.6 MB），
    /// UI 线程峰值 < 2 MB，GPU 最终持有全量数据。
    /// 空 Vec 表示流式上传完成。
    onion_skin_streaming_sender: Option<std::sync::mpsc::SyncSender<Vec<crate::NoteInstance>>>,
    /// 线程句柄
    thread_handle: Option<JoinHandle<()>>,
    /// 渲染完成的离屏纹理，供主线程读取
    pub latest_texture: Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    /// 双缓冲音符实例数据（UI线程写入，渲染线程读取）
    pub note_instances_buffer: Arc<SwappableBuffer<crate::NoteInstance>>,
    /// 洋葱皮生成进度缓冲（渲染线程写入，UI 线程读取并转发到进度窗口）
    onion_progress: Arc<Mutex<Vec<(String, f32)>>>,
}

impl WgpuRenderThread {
    /// 创建并启动渲染线程
    ///
    /// 采用离屏纹理架构：
    /// WGPU 渲染线程在后台将所有内容渲染到离屏纹理中，然后主线程将该纹理复制到 Surface。
    ///
    /// # 参数
    /// - `note_event_sender`: 音符事件发送端，UI 线程通过它发送增量更新事件。
    ///   必须由调用方持有，不能立即 drop（否则通道死信）。
    /// - `note_events_rx`: 音符事件接收端，渲染线程通过 `process_events()` 消费。
    pub fn spawn(
        device: wgpu::Device,
        queue: wgpu::Queue,
        texture_format: wgpu::TextureFormat,
        note_event_sender: std::sync::mpsc::Sender<crate::NoteEvent>,
        note_events_rx: std::sync::mpsc::Receiver<crate::NoteEvent>,
        note_instances_buffer: Arc<SwappableBuffer<crate::NoteInstance>>,
    ) -> anyhow::Result<Self> {
        tracing::info!("WgpuRenderThread::spawn - Starting render thread with offscreen texture");

        let stats = Arc::new(Mutex::new(RenderStats::default()));
        let running = Arc::new(AtomicBool::new(true));
        let (command_sender, command_receiver) = std::sync::mpsc::channel::<RenderCommand>();
        let latest_texture: Arc<Mutex<Option<Arc<wgpu::Texture>>>> = Arc::new(Mutex::new(None));
        let onion_progress: Arc<Mutex<Vec<(String, f32)>>> = Arc::new(Mutex::new(Vec::new()));

        // 洋葱皮流式上传 channel（容量 3 块 × 800 万实例/块 = 2400 万实例在途，最坏 ~384 MB）
        let (onion_skin_streaming_tx, onion_skin_streaming_rx) =
            std::sync::mpsc::sync_channel::<Vec<crate::NoteInstance>>(3);

        let stats_clone = Arc::clone(&stats);
        let running_clone = Arc::clone(&running);
        let latest_texture_clone = Arc::clone(&latest_texture);
        let note_instances_buffer_clone = Arc::clone(&note_instances_buffer);
        let onion_progress_clone = Arc::clone(&onion_progress);

        // 启动渲染线程
        let thread_handle = thread::spawn(move || {
            let ctx = RenderContext::new(device, queue, texture_format);
            let channels = RenderThreadChannels {
                running: running_clone,
                command_receiver,
                latest_texture_clone,
                stats_clone,
                note_events_rx,
                note_instances_buffer: note_instances_buffer_clone,
                onion_progress: onion_progress_clone,
                onion_skin_streaming_rx,
            };
            run_render_thread(ctx, channels);
        });

        Ok(Self {
            stats,
            running,
            command_sender: Some(command_sender),
            note_event_sender: Some(note_event_sender),
            onion_skin_streaming_sender: Some(onion_skin_streaming_tx),
            thread_handle: Some(thread_handle),
            latest_texture,
            note_instances_buffer,
            onion_progress,
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

    /// 克隆命令发送端（用于视频导出后台线程与渲染线程通信）
    pub fn try_clone_command_sender(&self) -> Option<std::sync::mpsc::Sender<RenderCommand>> {
        self.command_sender.clone()
    }

    /// 发送音符事件到渲染线程（增量更新通道）
    ///
    /// UI 线程编辑音符后调用此方法，渲染线程通过 `process_events()` 消费。
    /// 支持的事件：`Reset`（全量重载）、`Add`/`Update`/`UpdateMany`/`Remove`/`Clear`（增量）。
    ///
    /// 若渲染线程已关闭（sender 失效），事件被丢弃并记录警告。
    pub fn send_note_event(&self, event: crate::NoteEvent) {
        if let Some(ref sender) = self.note_event_sender
            && let Err(e) = sender.send(event)
        {
            tracing::warn!("Failed to send note event (render thread closed?): {}", e);
        }
    }

    /// 音符事件通道是否存活（sender 仍然存在且未关闭）
    pub fn is_note_event_channel_alive(&self) -> bool {
        self.note_event_sender.is_some()
    }

    /// 获取渲染统计
    pub fn stats(&self) -> RenderStats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 取出并清空洋葱皮生成进度缓冲（UI 线程每帧调用）
    pub fn drain_onion_progress(&self) -> Vec<(String, f32)> {
        self.onion_progress
            .lock()
            .map(|mut buf| std::mem::take(&mut *buf))
            .unwrap_or_default()
    }

    /// 将离屏渲染结果复制到 Surface 纹理
    ///
    /// 在主线程调用，将渲染线程生成的离屏纹理复制到当前 Surface。
    /// 如果尺寸不匹配或纹理不可用，则静默跳过。
    pub fn copy_offscreen_to_surface(
        &self,
        frame: &wgpu::SurfaceTexture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let texture_ref = self.latest_texture.try_lock().ok().and_then(|g| g.clone());

        let Some(texture) = texture_ref else {
            return;
        };

        if texture.width() != frame.texture.width() || texture.height() != frame.texture.height() {
            return;
        }

        puffin::profile_scope!("copy_offscreen_texture");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("copy_offscreen_texture_encoder"),
        });

        encoder.copy_texture_to_texture(
            texture.as_image_copy(),
            frame.texture.as_image_copy(),
            wgpu::Extent3d {
                width: texture.width(),
                height: texture.height(),
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));
    }

    /// 发送洋葱皮流式上传块（UI 线程分块构建后调用）
    ///
    /// 每块 ≤ 800 万实例（128 MB），sync_channel(3) 背压：
    /// channel 满时阻塞 UI 线程，等渲染线程消费后继续。
    /// 空 Vec 表示流式上传完成（渲染线程收到后调用 finish_streaming_upload）。
    pub fn send_onion_skin_chunk(&self, chunk: Vec<crate::NoteInstance>) {
        if let Some(ref sender) = self.onion_skin_streaming_sender
            && let Err(e) = sender.send(chunk)
        {
            tracing::warn!(
                "Failed to send onion skin chunk (render thread closed?): {}",
                e
            );
        }
    }

    /// 关闭渲染线程
    pub fn shutdown(mut self) {
        self.running.store(false, Ordering::Relaxed);

        // 关闭音符事件通道（drop sender 让渲染线程的 try_recv 收到 Disconnected 退出循环）
        self.note_event_sender.take();
        // 关闭洋葱皮流式通道
        self.onion_skin_streaming_sender.take();

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

    /// 回归测试：验证 NoteEvent channel 不会因 sender 被立即 drop 而死信。
    ///
    /// 之前 bug：`enable_separate_render_thread()` 中 `let (_tx, rx) = channel()`
    /// `_tx` 立即 dropped → `rx.try_recv()` 返回 `Disconnected` → `process_events()` 死信。
    /// 修复：sender 必须被持有（存储在 `WgpuRenderThread.note_event_sender`）。
    ///
    /// 本测试模拟修复后的模式：sender 存活在变量中，receiver 不应收到 Disconnected。
    #[test]
    fn test_note_event_channel_stays_alive_when_sender_held() {
        let (sender, receiver) = std::sync::mpsc::channel::<crate::NoteEvent>();

        // sender 被持有（模拟存储在 WgpuRenderThread 中），不 drop
        let _held_sender = sender;

        // receiver 不应收到 Disconnected（通道存活）
        // try_recv 在空通道上返回 Empty，而非 Disconnected
        match receiver.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("NoteEvent channel died: sender was dropped prematurely (regression)");
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // 期望行为：通道存活但暂无数据
            }
            Ok(_) => {
                panic!("Expected empty channel, but received an event");
            }
        }
    }

    /// 回归测试：验证 sender drop 后 receiver 收到 Disconnected（用于 shutdown 流程）
    #[test]
    fn test_note_event_channel_disconnects_on_sender_drop() {
        let (sender, receiver) = std::sync::mpsc::channel::<crate::NoteEvent>();
        drop(sender); // 模拟 shutdown 中 `note_event_sender.take()`

        match receiver.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // 期望行为：sender drop 后通道断开，渲染线程可据此退出循环
            }
            _ => {
                panic!("Expected Disconnected after sender dropped");
            }
        }
    }

    /// 验证 NoteEvent 能通过 channel 正确传递（端到端通道健康度）
    #[test]
    fn test_note_event_channel_delivers_event() {
        let (sender, receiver) = std::sync::mpsc::channel::<crate::NoteEvent>();

        // 模拟 UI 线程发送 Clear 事件
        sender
            .send(crate::NoteEvent::Clear)
            .expect("发送 Clear 事件失败");

        // 模拟渲染线程 process_events 消费
        match receiver.try_recv() {
            Ok(crate::NoteEvent::Clear) => {
                // 期望行为：事件正确传递
            }
            other => {
                panic!("Expected NoteEvent::Clear, got {:?}", other);
            }
        }
    }
}
