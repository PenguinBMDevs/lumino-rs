use crate::host::Host;
use crate::message::Message;
use crate::statusbar::performance::PerfData;
use crate::window;

impl Host {
    /// 帧准备：处理事件和计算 FPS，收集性能数据
    pub(super) fn process_frame_preparation(&mut self) {
        // 处理待处理的事件队列（合并后的）
        // 这样可以确保同一帧内的多个事件被合并处理，减少 UI 重建次数
        self.process_pending_events();

        // 更新播放状态和自动滚动
        self.update_playback_state();

        // 更新 GPU 帧耗时（从渲染线程统计）
        self.last_gpu_frame_time_ms = self
            .render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.stats().last_frame_time_ms as f32)
            .unwrap_or(0.0);

        // 计算 FPS 并收集性能数据
        self.frame_count += 1;
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_fps_update);

        if elapsed.as_millis() >= super::FPS_UPDATE_INTERVAL_MS {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();

            // 收集 CPU、内存、GPU 数据
            let cpu_usage = self.cpu_monitor.usage();
            let memory_mb =
                lumino_core::memory_monitor::platform::get_current_rss() as f32 / (1024.0 * 1024.0);
            let gpu_frame_time = self.last_gpu_frame_time_ms;

            let perf_data = PerfData::new(fps, cpu_usage, memory_mb, gpu_frame_time);
            self.root.update(window::Event::perf_update(perf_data));

            // 保持向后兼容：仍然发送 FPS 更新给 Window 状态
            self.root.update(window::Event::fps_update(fps));

            self.frame_count = 0;
            self.last_fps_update = now;
        }

        self.last_frame_time = now;

        // 更新模式切换按钮的弹簧物理动画
        if self.root.state.toggle_animation.active {
            self.root.update(Message::AnimationTick);
            self.window_ctx.window.request_redraw();
        }
    }

    /// 更新播放状态
    pub(super) fn update_playback_state(&mut self) {
        if let Some(tick) = self.root.update_playback() {
            let old_pos = self.root.editor.playback_position;
            self.root.editor.playback_position = tick;
            if self.root.is_playing() {
                // 始终更新自动滚动（侧效果：设置 scroll_x）
                self.root.editor.update_auto_scroll(tick);
                // 播放时总是请求重绘并标记 UI 脏，确保播放指示线位置更新。
                // 关键：即使自动滚动不触发（如循环回绕后指示线回到起点），
                // 也必须重建 iced canvas UI 使指示线在新位置绘制。
                if (tick - old_pos).abs() > f32::EPSILON {
                    self.ui_dirty = true;
                }
                self.window_ctx.window.request_redraw();
            } else if self.root.editor.update_auto_scroll(tick) {
                // 非播放状态：仅当自动滚动触发时才重绘（旧行为）
                self.window_ctx.window.request_redraw();
            }
        }
    }

    /// 更新光标位置（用于音符预览）
    pub(super) fn update_cursor_for_preview(&mut self) {
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            self.root
                .update_editor_cursor(self.window_ctx.cursor_position);
        }
    }
}
