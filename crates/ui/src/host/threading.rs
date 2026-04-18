use crate::host::Host;
use crate::{RenderThreadHandle, WgpuRenderThread, spawn_render_thread};
use std::sync::Arc;

impl Host {
    /// 启用独立渲染线程模式
    ///
    /// 这会将WGPU渲染从UI线程分离到独立线程，提高UI响应性
    pub fn enable_render_thread(&mut self) {
        if self.render_thread.is_some() {
            return;
        }

        let (mut handle, receiver) = RenderThreadHandle::new();
        let stats = Arc::clone(&handle.stats);

        // 启动渲染线程
        let thread_handle = spawn_render_thread(receiver, stats);

        // 存储线程句柄
        handle.thread_handle = Some(thread_handle);

        self.render_thread = Some(handle);
        self.use_render_thread = true;

        tracing::info!("Host: Render thread enabled");
    }

    /// 禁用独立渲染线程模式
    pub fn disable_render_thread(&mut self) {
        if let Some(handle) = self.render_thread.take() {
            handle.shutdown();
            self.use_render_thread = false;
            tracing::info!("Host: Render thread disabled");
        }
    }

    /// 获取渲染线程统计信息
    pub fn render_stats(&self) -> Option<crate::RenderStats> {
        self.render_thread.as_ref().map(|h| h.stats())
    }

    /// 启用真正的分离渲染线程（新架构）
    ///
    /// 这会将所有 WGPU 渲染（音符、网格、键盘、标尺）从 UI 线程完全分离
    pub fn enable_separate_render_thread(&mut self) {
        if self.wgpu_render_thread.is_some() {
            return;
        }

        // 创建音符事件通道
        let (tx, rx) = std::sync::mpsc::channel();

        // 启动 WGPU 渲染线程
        match WgpuRenderThread::spawn(self.device.clone(), self.queue.clone(), self.format, rx) {
            Ok(thread) => {
                self.wgpu_render_thread = Some(thread);
                self.note_events_tx = Some(tx);
                self.use_separate_render_thread = true;
                tracing::info!("Host: Separate WGPU render thread enabled");
            }
            Err(e) => {
                tracing::error!("Host: Failed to start separate render thread: {}", e);
            }
        }
    }

    /// 禁用分离渲染线程
    pub fn disable_separate_render_thread(&mut self) {
        if let Some(thread) = self.wgpu_render_thread.take() {
            thread.shutdown();
            self.use_separate_render_thread = false;
            self.note_events_tx = None;
            tracing::info!("Host: Separate WGPU render thread disabled");
        }
    }

    /// 获取分离渲染线程统计
    pub fn separate_render_stats(&self) -> Option<crate::WgpuRenderStats> {
        self.wgpu_render_thread.as_ref().map(|t| t.stats())
    }
}
