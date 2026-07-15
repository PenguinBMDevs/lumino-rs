//! 事件循环控制流模块

use std::time::Duration;

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::runner::inner::RunnerInner;
use lumino_ui::state::root_state::DialogType;

impl RunnerInner {
    /// 内存监控对话框的实时刷新间隔（毫秒）。
    ///
    /// 利用 iced 的自动重绘检测（状态变更/动画/播放头都会主动 `request_redraw`），
    /// 事件循环在空闲时休眠（[`ControlFlow::Wait`]）。仅当内存监控对话框打开时，
    /// 用 [`ControlFlow::WaitUntil`] 给它一个低频率心跳，既保持数字实时刷新，
    /// 又避免 `Poll` 模式下的 100% GPU 空转。
    const MEMORY_MONITOR_REFRESH_MS: u64 = 300;

    /// 导出/视频对话框进度轮询间隔（毫秒）。
    ///
    /// 导出线程仅通过通道发送进度，不会主动 `emit` 唤醒事件循环
    /// （进度消费位于 `about_to_wait_forward_export_progress`）。
    /// 旧 `Poll` 模式每帧排空通道，切到 `Wait` 后若循环休眠进度条会冻住。
    /// 因此当导出/视频对话框打开时，用 [`ControlFlow::WaitUntil`] 以 50ms 心跳
    /// 维持进度刷新——只在用户主动、短生命的导出期间生效，空闲时仍为纯 `Wait`。
    const EXPORT_PROGRESS_POLL_MS: u64 = 50;

    /// 处理事件循环控制流策略
    pub(crate) fn handle_control_flow(&mut self, event_loop: &ActiveEventLoop) {
        // 默认进入 Wait 模式：事件循环休眠，仅在收到事件或 `request_redraw` 时唤醒重绘。
        // 这是 iced 的推荐用法——UI 在状态变更时通过 Notifier / 事件处理器主动
        // `request_redraw`，无需 Poll 持续空刷（那是 GPU 占用爆表的真正根因）。
        let now = std::time::Instant::now();
        let mut next_wake: Option<std::time::Instant> = None;

        let has = |ty: DialogType| self.window_state.dialog_manager.has_dialog_type(ty);

        if has(DialogType::MemoryMonitor) {
            next_wake = Some(now + Duration::from_millis(Self::MEMORY_MONITOR_REFRESH_MS));
        }
        if has(DialogType::ExportProgress) || has(DialogType::VideoExport) {
            let deadline = now + Duration::from_millis(Self::EXPORT_PROGRESS_POLL_MS);
            next_wake = Some(match next_wake {
                Some(t) => t.min(deadline),
                None => deadline,
            });
        }

        match next_wake {
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}
