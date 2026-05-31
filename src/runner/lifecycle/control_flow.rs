//! 事件循环控制流模块

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理事件循环控制流策略
    pub(crate) fn handle_control_flow(&mut self, event_loop: &ActiveEventLoop) {
        // 始终使用 Poll 模式实现持续刷新
        // 这样 UI 会实时更新，而不是只在鼠标移动时刷新
        event_loop.set_control_flow(ControlFlow::Poll);

        // 在 Poll 模式下，每次循环都请求重绘
        self.window_state.window.request_redraw();
    }
}
