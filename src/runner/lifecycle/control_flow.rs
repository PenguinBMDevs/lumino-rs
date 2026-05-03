//! 事件循环控制流模块

use std::time::{Duration, Instant};

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理事件循环控制流策略
    pub(crate) fn handle_control_flow(&mut self, event_loop: &ActiveEventLoop) {
        let is_playing = self.window_state.window.ui().is_playing();
        let is_test_active = self
            .test_state
            .test_mode_state
            .as_ref()
            .map(|s| s.active)
            .unwrap_or(false);
        let should_poll = is_playing || is_test_active;

        if should_poll {
            event_loop.set_control_flow(ControlFlow::Poll);
            self.window_state.window.request_redraw();
        } else if self.test_state.log_memory_usage {
            let next_log = self
                .test_state
                .last_memory_log
                .map(|last| last + Duration::from_millis(2000))
                .unwrap_or_else(Instant::now);
            event_loop.set_control_flow(ControlFlow::WaitUntil(next_log));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}
