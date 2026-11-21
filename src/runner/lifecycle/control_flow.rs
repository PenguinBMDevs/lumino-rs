//! 事件循环控制流模块

use std::time::Duration;

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::runner::inner::RunnerInner;
use lumino_ui::state::root_state::DialogType;

impl RunnerInner {
    /// 内存监控对话框的实时刷新间隔（毫秒）。
    ///
    /// 利用 上一代 的自动重绘检测（状态变更/动画/播放头都会主动 `request_redraw`），
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
        // 这是推荐用法——UI 在状态变更时通过 Notifier / 事件处理器主动
        // `request_redraw`，无需 Poll 持续空刷（那是 GPU 占用爆表的真正根因）。
        let now = std::time::Instant::now();

        let has = |ty: DialogType| self.window_state.dialog_manager.has_dialog_type(ty);

        // 所有唤醒候选一次性收集：flatten 丢 None，min 取最早时刻，
        // 等价于原手写的 t.min(deadline) 链。`bool::then` 惰性求值，
        // 周期时间仍只在对应对话框打开时构造；检查顺序保持不变。
        let next_wake = earliest_deadline([
            has(DialogType::MemoryMonitor)
                .then(|| now + Duration::from_millis(Self::MEMORY_MONITOR_REFRESH_MS)),
            (has(DialogType::ExportProgress) || has(DialogType::VideoExport))
                .then(|| now + Duration::from_millis(Self::EXPORT_PROGRESS_POLL_MS)),
            // GPU 检测超时冷却：到点必须唤醒一次，否则按钮要等下次鼠标移动才恢复
            // （`Wait` 模式下没有周期 tick，冷却结束这件事本身不会产生事件）。
            self.gpu_check.next_wake(now),
            // 周期自动保存：到点必须唤醒一次（Wait 模式下定时器本身不产生事件）。
            // 未启用时 `autosave_next` 为 None，不产生任何额外唤醒。
            self.autosave_next,
            self.startup_prompt_at,
        ]);

        event_loop.set_control_flow(control_flow_for_wake(next_wake));
    }
}

/// 取所有唤醒候选中最早的一个；无候选返回 `None`（上层转 [`ControlFlow::Wait`]）。
///
/// `Instant::min` 与比较顺序无关，故数组顺序只影响求值时机、不影响结果；
/// 抽成纯函数以便覆盖空/单/乱序/相同/已过期五种边界。
fn earliest_deadline(candidates: [Option<std::time::Instant>; 5]) -> Option<std::time::Instant> {
    candidates.into_iter().flatten().min()
}

/// 唤醒时刻 → 事件循环控制流策略：无唤醒休眠，有唤醒定时等待。
fn control_flow_for_wake(next_wake: Option<std::time::Instant>) -> ControlFlow {
    next_wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil)
}

#[cfg(test)]
mod tests {
    use super::{ControlFlow, control_flow_for_wake, earliest_deadline};
    use std::time::{Duration, Instant};

    #[test]
    fn no_candidates_yields_wait() {
        assert_eq!(earliest_deadline([None, None, None, None, None]), None);
        assert!(matches!(control_flow_for_wake(None), ControlFlow::Wait));
    }

    #[test]
    fn single_candidate_is_returned() {
        let t = Instant::now();
        assert_eq!(
            earliest_deadline([None, Some(t), None, None, None]),
            Some(t)
        );
    }

    #[test]
    fn out_of_order_candidates_pick_earliest() {
        let base = Instant::now();
        let earliest = base + Duration::from_millis(10);
        let latest = base + Duration::from_millis(50);
        let middle = base + Duration::from_millis(30);
        assert_eq!(
            earliest_deadline([Some(latest), Some(earliest), None, Some(middle), None]),
            Some(earliest)
        );
    }

    #[test]
    fn equal_deadlines_keep_the_same_instant() {
        let t = Instant::now() + Duration::from_millis(100);
        assert_eq!(
            earliest_deadline([Some(t), None, Some(t), None, Some(t)]),
            Some(t)
        );
    }

    #[test]
    fn past_deadline_is_not_discarded() {
        let past = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            earliest_deadline([None, None, None, Some(past), None]),
            Some(past)
        );
    }

    #[test]
    fn some_deadline_maps_to_wait_until() {
        let t = Instant::now() + Duration::from_millis(20);
        assert!(matches!(
            control_flow_for_wake(Some(t)),
            ControlFlow::WaitUntil(got) if got == t
        ));
    }
}
