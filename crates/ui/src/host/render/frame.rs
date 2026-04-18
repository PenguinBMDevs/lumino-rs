use crate::host::Host;
use crate::window;

impl Host {
    /// 帧准备：处理事件和计算 FPS
    pub(super) fn process_frame_preparation(&mut self) {
        // 处理待处理的事件队列（合并后的）
        // 这样可以确保同一帧内的多个事件被合并处理，减少 UI 重建次数
        self.process_pending_events();

        // 计算 FPS
        self.frame_count += 1;
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_fps_update);

        if elapsed.as_millis() >= super::FPS_UPDATE_INTERVAL_MS {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.root.update(window::Event::fps_update(fps));
            self.frame_count = 0;
            self.last_fps_update = now;
        }

        self.last_frame_time = now;
    }

    /// 更新播放状态
    pub(super) fn update_playback_state(&mut self) {
        if let Some(tick) = self.root.update_playback() {
            self.root.editor.playback_position = tick;
            // 播放时自动滚动会改变 scroll_x，仅请求重绘（canvas/WGPU层处理）
            if self.root.editor.update_auto_scroll(tick) {
                self.window.request_redraw();
            }
        }
    }

    /// 更新光标位置（用于音符预览）
    pub(super) fn update_cursor_for_preview(&mut self) {
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            self.root.update_editor_cursor(self.cursor_position);
        }
    }
}
