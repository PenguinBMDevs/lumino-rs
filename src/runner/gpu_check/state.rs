//! 手动 GPU 检测的会话状态机（纯逻辑；时间由调用方注入，便于单测）。
//!
//! 单点裁决「能不能开始」「结束后冷却多久 / 是否锁定」，并投影成 UI 快照。
//! 状态放在 Runner（会话级）而不是 `SettingsPanel`：设置面板每次打开都会重建
//! （`Root::new` → `SettingsPanel::new`），面板内状态撑不过关窗 / 开窗。
//!
//! 检测结果的展示生命周期绑定兼容性页：用户切到其他设置页后，Runner 调用
//! [`GpuCheckState::clear_result`] 作废上一次结果；在飞、冷却与会话锁定不受影响。

use std::time::{Duration, Instant};

use lumino_ui::state::root_state::{
    GpuCheckOutcome, GpuCheckRetryGate, GpuCheckUiResult, GpuCheckUiState,
};

/// 超时后的冷却时长：期间禁止重试，避免在疑似挂起的驱动上立刻叠加第二个检测。
pub(crate) const TIMEOUT_COOLDOWN: Duration = Duration::from_secs(3);

/// 连续超时达到该次数后锁定到应用重启。
///
/// 每次超时都会遗留一个无法回收的检测线程与一套 wgpu 实例（Rust 无线程取消），
/// 冷却只能降低点击频率，挡不住「每 3 秒点一次」的累积；锁定是唯一的硬止损。
pub(crate) const MAX_CONSECUTIVE_TIMEOUTS: u32 = 2;

/// 手动 GPU 检测会话状态（唯一真源）
#[derive(Debug, Default)]
pub(crate) struct GpuCheckState {
    /// 已有检测在飞：拒绝重入
    in_flight: bool,
    /// 连续超时次数（非超时结果清零）
    consecutive_timeouts: u32,
    /// 会话锁定：连续超时后本次运行不再允许检测
    locked: bool,
    /// 冷却截止时刻
    cooldown_until: Option<Instant>,
    /// 最近一次结果（只在兼容性页当前可见时保留；离开该页即作废）
    last: Option<GpuCheckUiResult>,
}

impl GpuCheckState {
    /// 尝试开始一次检测；返回 `false` 表示被拦截（在飞 / 冷却 / 会话锁定）。
    pub(crate) fn try_begin(&mut self, now: Instant) -> bool {
        if self.in_flight || self.locked || self.cooldown_active(now) {
            return false;
        }
        self.in_flight = true;
        true
    }

    /// 记录一次检测结束，并按结论推进冷却 / 锁定。
    pub(crate) fn finish(&mut self, outcome: GpuCheckOutcome, detail: String, now: Instant) {
        self.in_flight = false;
        if outcome == GpuCheckOutcome::TimedOut {
            self.consecutive_timeouts += 1;
            if self.consecutive_timeouts >= MAX_CONSECUTIVE_TIMEOUTS {
                self.locked = true;
                self.cooldown_until = None;
            } else {
                self.cooldown_until = Some(now + TIMEOUT_COOLDOWN);
            }
        } else {
            self.consecutive_timeouts = 0;
            self.cooldown_until = None;
        }
        self.last = Some(GpuCheckUiResult { outcome, detail });
    }

    /// 作废最近一次结果（用户离开兼容性页时调用）。
    ///
    /// 只清「上一次检测信息」，不动在飞标记与冷却 / 锁定门控：检测线程无法取消，
    /// 重试门控必须继续保护驱动；回到兼容性页时，在飞检测仍显示 `Running`，
    /// 已完成的历史结果则回到 `Idle`。
    ///
    /// 返回是否真的清掉了结果（调用方据此决定是否强制刷新 UI）。
    pub(crate) fn clear_result(&mut self) -> bool {
        self.last.take().is_some()
    }

    /// 到点推进：冷却结束返回 `true`（调用方据此刷新 UI）。
    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        if self.cooldown_until.is_some_and(|deadline| now >= deadline) {
            self.cooldown_until = None;
            return true;
        }
        false
    }

    /// 当前重试门控（锁定优先于冷却）。
    pub(crate) fn retry_gate(&self, now: Instant) -> GpuCheckRetryGate {
        if self.locked {
            GpuCheckRetryGate::Locked
        } else if self.cooldown_active(now) {
            GpuCheckRetryGate::Cooling
        } else {
            GpuCheckRetryGate::Ready
        }
    }

    /// 投影成设置面板可展示的状态（冷却 / 锁定期间结果仍然可见）。
    pub(crate) fn ui_state(&self) -> GpuCheckUiState {
        if self.in_flight {
            return GpuCheckUiState::Running;
        }
        match &self.last {
            Some(result) => GpuCheckUiState::Done(result.clone()),
            None => GpuCheckUiState::Idle,
        }
    }

    /// 下一次需要唤醒事件循环的时刻（冷却截止）；锁定 / 可重试时为 `None`。
    pub(crate) fn next_wake(&self, now: Instant) -> Option<Instant> {
        self.cooldown_until.filter(|deadline| *deadline > now)
    }

    /// 冷却是否仍在生效
    fn cooldown_active(&self, now: Instant) -> bool {
        self.cooldown_until.is_some_and(|deadline| now < deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeout_once(state: &mut GpuCheckState, now: Instant) {
        assert!(state.try_begin(now), "空闲状态下检测应被接受");
        state.finish(GpuCheckOutcome::TimedOut, "检测超时".into(), now);
    }

    #[test]
    fn test_first_click_accepted_and_marks_running() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        assert_eq!(state.ui_state(), GpuCheckUiState::Idle);
        assert!(state.try_begin(now));
        assert_eq!(state.ui_state(), GpuCheckUiState::Running);
        assert_eq!(state.retry_gate(now), GpuCheckRetryGate::Ready);
    }

    /// 重入缺口的最小断言：在飞期间第二次点击必须被拒绝
    #[test]
    fn test_second_click_while_running_is_rejected() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        assert!(state.try_begin(now));
        assert!(
            !state.try_begin(now),
            "检测进行中不得并发第二个检测（会叠加线程与 wgpu 实例）"
        );
        assert_eq!(state.ui_state(), GpuCheckUiState::Running);
    }

    #[test]
    fn test_timeout_starts_cooldown_and_blocks_retry() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        timeout_once(&mut state, now);

        assert_eq!(state.retry_gate(now), GpuCheckRetryGate::Cooling);
        assert!(
            !state.try_begin(now + Duration::from_secs(1)),
            "冷却期内必须拒绝重试"
        );
        // 结果仍然可见（不能因为冷却把刚失败的结果藏起来）
        assert_eq!(
            state.ui_state(),
            GpuCheckUiState::Done(GpuCheckUiResult {
                outcome: GpuCheckOutcome::TimedOut,
                detail: "检测超时".into(),
            })
        );
    }

    #[test]
    fn test_cooldown_expires_and_reenables_button() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        timeout_once(&mut state, now);

        assert!(!state.tick(now + TIMEOUT_COOLDOWN - Duration::from_millis(1)));
        assert_eq!(state.retry_gate(now), GpuCheckRetryGate::Cooling);

        let after = now + TIMEOUT_COOLDOWN;
        assert!(state.tick(after), "冷却到点必须返回 true 以触发 UI 刷新");
        assert_eq!(state.retry_gate(after), GpuCheckRetryGate::Ready);
        assert!(state.try_begin(after), "冷却结束后允许重试");
        assert_eq!(state.next_wake(after), None);
    }

    /// 连续超时 → 会话锁定：冷却挡不住"每 3 秒点一次"，必须硬止损
    #[test]
    fn test_second_consecutive_timeout_locks_session_until_restart() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        timeout_once(&mut state, now);

        let after_cooldown = now + TIMEOUT_COOLDOWN;
        assert!(state.tick(after_cooldown));
        timeout_once(&mut state, after_cooldown);

        let much_later = after_cooldown + Duration::from_secs(600);
        assert_eq!(state.retry_gate(much_later), GpuCheckRetryGate::Locked);
        assert!(!state.try_begin(much_later), "锁定后不得再启动检测");
        assert!(!state.tick(much_later), "锁定不是冷却，不应产生唤醒");
        assert_eq!(state.next_wake(much_later), None);
    }

    /// 一次成功必须清零连续超时计数（否则偶发超时会累积到锁定）
    #[test]
    fn test_success_resets_timeout_counter() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        timeout_once(&mut state, now);

        let after_cooldown = now + TIMEOUT_COOLDOWN;
        assert!(state.tick(after_cooldown));
        assert!(state.try_begin(after_cooldown));
        state.finish(GpuCheckOutcome::Passed, "检测通过".into(), after_cooldown);
        assert_eq!(state.retry_gate(after_cooldown), GpuCheckRetryGate::Ready);
        assert_eq!(state.next_wake(after_cooldown), None);

        // 计数已清零：再超时一次只进入冷却，不会直接锁定
        let later = after_cooldown + Duration::from_secs(10);
        timeout_once(&mut state, later);
        assert_eq!(state.retry_gate(later), GpuCheckRetryGate::Cooling);
    }

    #[test]
    fn test_next_wake_only_points_at_active_cooldown() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        assert_eq!(state.next_wake(now), None, "空闲时无需唤醒");

        timeout_once(&mut state, now);
        assert_eq!(
            state.next_wake(now),
            Some(now + TIMEOUT_COOLDOWN),
            "冷却期必须给出唤醒时刻，否则按钮要等下次鼠标移动才恢复"
        );
        assert_eq!(
            state.next_wake(now + TIMEOUT_COOLDOWN),
            None,
            "已到点的不再产生唤醒（避免忙循环）"
        );
    }

    /// 离开兼容性页只隐藏结果：冷却门控不能被顺手解除
    #[test]
    fn test_clear_result_hides_done_but_keeps_retry_gate() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        timeout_once(&mut state, now);
        assert_eq!(state.retry_gate(now), GpuCheckRetryGate::Cooling);

        assert!(state.clear_result(), "已有结果时必须报告清除成功");
        assert_eq!(state.ui_state(), GpuCheckUiState::Idle);
        assert_eq!(
            state.retry_gate(now),
            GpuCheckRetryGate::Cooling,
            "清结果不得解除冷却（驱动保护仍要生效）"
        );
        assert!(!state.clear_result(), "重复清除不得再次报告成功");
    }

    /// 会话锁定不能被「离开页面」解除：锁定是驱动保护的硬止损
    #[test]
    fn test_clear_result_keeps_session_lock() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        timeout_once(&mut state, now);
        let after = now + TIMEOUT_COOLDOWN;
        assert!(state.tick(after));
        timeout_once(&mut state, after);
        assert_eq!(state.retry_gate(after), GpuCheckRetryGate::Locked);

        assert!(state.clear_result());
        assert_eq!(state.ui_state(), GpuCheckUiState::Idle);
        assert_eq!(
            state.retry_gate(after),
            GpuCheckRetryGate::Locked,
            "清结果不得解除会话锁定"
        );
        assert!(!state.try_begin(after), "锁定后仍禁止重新检测");
    }

    /// 在飞检测不能被「离开页面」取消；回来后仍应显示检测中
    #[test]
    fn test_clear_result_keeps_in_flight_running() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        assert!(state.try_begin(now));
        assert!(!state.clear_result(), "没有历史结果时清除应返回 false");
        assert_eq!(
            state.ui_state(),
            GpuCheckUiState::Running,
            "清结果只清历史结果，不得假装检测已结束"
        );
    }

    /// 清除历史结果不得影响重新检测与结果展示
    #[test]
    fn test_check_can_rerun_after_result_cleared() {
        let now = Instant::now();
        let mut state = GpuCheckState::default();
        assert!(state.try_begin(now));
        state.finish(GpuCheckOutcome::Passed, "检测通过".into(), now);
        assert!(state.clear_result());
        assert_eq!(state.ui_state(), GpuCheckUiState::Idle);

        assert!(state.try_begin(now), "清除历史结果不得影响重试");
        state.finish(GpuCheckOutcome::Passed, "再次通过".into(), now);
        assert_eq!(
            state.ui_state(),
            GpuCheckUiState::Done(GpuCheckUiResult {
                outcome: GpuCheckOutcome::Passed,
                detail: "再次通过".into(),
            })
        );
    }
}
