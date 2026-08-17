//! 渲染线程管理 —— 从 Host 拆出的分离渲染线程相关方法
//!
//! 管理 WGPU 独立渲染线程的启停、音符事件通道、线程统计等。

use super::Host;

impl Host {
    /// 启用真正的分离渲染线程（新架构）
    ///
    /// 这会将所有 WGPU 渲染（音符、网格、键盘、标尺）从 UI 线程完全分离
    pub fn enable_separate_render_thread(&mut self) {
        if self.render_ctx.wgpu_render_thread.is_some() {
            return;
        }

        // 创建音符事件通道（sender 传递给 WgpuRenderThread 持有，避免立即 drop 导致死信）
        // 之前 bug：`let (_tx, rx) = channel()` 中 `_tx` 立即 dropped，
        // 渲染线程的 `process_events()` 永远收到 Disconnected，增量更新通道死信。
        // 修复：sender 传递给 WgpuRenderThread::spawn 存储，通过 send_note_event() 暴露。
        let (note_event_tx, note_event_rx) = std::sync::mpsc::channel();

        // 启动 WGPU 渲染线程
        match crate::WgpuRenderThread::spawn(
            self.render_ctx.device.clone(),
            self.render_ctx.queue.clone(),
            self.render_ctx.format,
            note_event_tx,
            note_event_rx,
            std::sync::Arc::clone(&self.render_ctx.render_cache.note_instances_buffer),
        ) {
            Ok(thread) => {
                self.render_ctx.wgpu_render_thread = Some(thread);
                tracing::info!("Host: Separate WGPU render thread enabled");
            }
            Err(e) => {
                tracing::error!("Host: Failed to start separate render thread: {}", e);
            }
        }
    }

    /// 禁用分离渲染线程
    pub fn disable_separate_render_thread(&mut self) {
        if let Some(thread) = self.render_ctx.wgpu_render_thread.take() {
            thread.shutdown();
            tracing::info!("Host: Separate WGPU render thread disabled");
        }
    }

    /// 发送音符事件到渲染线程（增量更新通道）
    ///
    /// UI 线程编辑音符后调用此方法，渲染线程通过 `process_events()` 消费。
    /// 仅在分离渲染线程模式下有效；单线程模式下此方法为空操作（数据直接走 GpuNoteBuffer）。
    ///
    /// 支持的事件：
    /// - `Reset(Vec<NoteInstance>)`：全量重载音符（MIDI 加载后）
    /// - `Add(NoteInstance)`：添加单个音符
    /// - `Update { index, instance }`：更新单个音符
    /// - `UpdateMany { start_index, instances }`：批量更新
    /// - `Remove(index)`：删除单个音符
    /// - `Clear`：清空所有音符
    pub fn send_note_event_to_render_thread(&self, event: lumino_gfx::NoteEvent) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_note_event(event);
        }
    }

    /// 检查渲染线程音符事件通道是否存活
    ///
    /// 用于测试验证：`enable_separate_render_thread()` 后通道应存活。
    pub fn is_note_event_channel_alive(&self) -> bool {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .is_some_and(|t| t.is_note_event_channel_alive())
    }

    /// 发送洋葱皮流式消息到渲染线程（全量会话 / TrackDelta / SetViewState / 预览）
    ///
    /// 统一全量渲染（2026-08-06）：主音轨与洋葱皮共用此通道——
    /// - `SetViewState`：切轨/静音零重传
    /// - `PreviewInstances`：Drawing/hover/i2m 预览实例
    pub fn send_onion_skin_msg_to_render_thread(&self, msg: lumino_gfx::OnionSkinStreamMsg) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_onion_skin_msg(msg);
        }
    }

    /// 获取分离渲染线程统计
    pub fn separate_render_stats(&self) -> Option<crate::WgpuRenderStats> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.stats())
    }

    /// 克隆渲染线程命令发送端（用于视频导出后台线程与渲染线程通信）
    pub fn render_command_sender(
        &self,
    ) -> Option<std::sync::mpsc::Sender<lumino_gfx::render_thread::RenderCommand>> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .and_then(|t| t.try_clone_command_sender())
    }
}
