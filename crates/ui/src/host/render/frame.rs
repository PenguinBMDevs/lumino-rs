use crate::host::Host;
use crate::message::Message;
use crate::statusbar::performance::PerfData;
use crate::window;

impl Host {
    /// 帧准备：处理事件和计算 FPS，收集性能数据
    pub(super) fn process_frame_preparation(&mut self) {
        puffin::profile_function!();

        // 处理待处理的事件队列（合并后的）
        // 这样可以确保同一帧内的多个事件被合并处理，减少 UI 重建次数
        self.process_pending_events();

        // 清除窗口最大化/还原保护标志（已在 handle_sidebar_event 中阻止路由切换）
        self.root.window_resize_guard = false;

        // 更新播放状态和自动滚动
        self.update_playback_state();

        // 确保工程走带视图的 max_tick_end 缓存是最新的（用于滚动范围计算）
        self.root.arrangement_max_tick_end();

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

        let interval_ms = self.root.settings().monitor_refresh_interval_ms as u128;
        if elapsed.as_millis() >= interval_ms {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();

            // 收集 CPU、内存、GPU 数据
            let cpu_usage = self.cpu_monitor.usage();
            // 内存取分配器追踪总量（含 GPU 资源）与 RSS 的较大值，避免遗漏未追踪的堆外占用。
            let rss_mb =
                lumino_memory_monitor::platform::get_current_rss() as f64 / (1024.0 * 1024.0);
            let tracked_mb = crate::statusbar::performance::aggregate_memory_mb();
            let memory_mb = tracked_mb.max(rss_mb) as f32;
            let gpu_frame_time = self.last_gpu_frame_time_ms;

            let perf_data = PerfData::new(fps, cpu_usage, memory_mb, gpu_frame_time);
            self.route_message(window::Event::perf_update(perf_data));

            // 保持向后兼容：仍然发送 FPS 更新给 Window 状态
            self.route_message(window::Event::fps_update(fps));

            self.frame_count = 0;
            self.last_fps_update = now;
        }

        self.last_frame_time = now;

        // 更新模式切换按钮的弹簧物理动画、平滑滚动动画和框选框动画
        let has_selection_anim = self
            .root
            .editor
            .selection_box_anim
            .get()
            .is_some_and(|s| !s.converged);
        let needs_animation = self.root.state.toggle_animation.active
            || self.root.editor.editor_state.view.smooth_scroll.active
            || has_selection_anim;
        if needs_animation {
            self.route_message(Message::AnimationTick);
            self.window_ctx.window.request_redraw();
        }

        // 关键：存在 pending 异步提交时，即使没有动画也要触发 AnimationTick，
        // 否则 `poll_async_commit` 不会被调用，导致撤销/重做等快捷键被阻塞，
        // 同时空间索引也无法及时重建。
        if self.root.editor.has_pending_drag() {
            self.route_message(Message::AnimationTick);
            self.window_ctx.window.request_redraw();
        }

        // 音轨拖拽排序候选进行中：每帧触发 AnimationTick 驱动长按计时，
        // 并持续重绘以刷新插入位置指示线（长按激活后同样需要逐帧重绘）。
        if self.root.sidebar.track_reorder_pending() {
            self.route_message(Message::AnimationTick);
            self.window_ctx.window.request_redraw();
        }
    }

    /// 更新播放状态
    pub(super) fn update_playback_state(&mut self) {
        puffin::profile_function!();
        if let Some(tick) = self.root.update_playback() {
            let old_pos = self.root.editor.playback_position;
            self.root.editor.playback_position = tick;
            if self.root.is_playing() {
                // 始终更新自动滚动（侧效果：设置 scroll_x）
                self.root.editor.update_auto_scroll(tick);
                // 工程走带视图也应用相同的自动滚动配置
                self.root.update_arrangement_auto_scroll(tick);
                // 更新播放期间琴键洋葱皮颜色（实时检测音符并着色键盘）
                self.root.editor.update_playback_key_colors();
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
