//! REND-019 PR-C：重连状态机全迁移路径测试（Mock `ReconnectOps`，无 GPU 依赖）。
//!
//! 覆盖（§1.4 S1 的验收口径）：
//! - 完整成功路径 `Stop → Recreate → Infra → Replay → Legacy → Restart → Recovered`；
//! - 设备重建连续失败 → 预算耗尽 → `Failed`（且 `on_failed` 恰调用一次）；
//! - 基础设施步失败 → 整轮降级重试 → 第二轮成功恢复；
//! - 等待 worker 期间的步进幂等（不重复发起重建）。

use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Duration;

use lumino_gfx::GpuEpoch;

use super::{MAX_RECONNECT_ATTEMPTS, ReconnectOps, ReconnectSession, ReconnectState};

/// 脚本化的重建结果（每次 spawn 弹出一个）。
type Scripted = VecDeque<Result<(GpuEpoch, u32), String>>;

/// Mock 操作面：记录调用序列；通过预置 sender 控制 worker 结果时机。
struct MockOps {
    calls: Vec<&'static str>,
    scripted: Scripted,
    /// 已发放的 worker 结果 sender（保活，模拟"稍后返回"）
    agents: Vec<mpsc::Sender<Result<(GpuEpoch, u32), String>>>,
    /// 前 N 次 install 调用强制失败（降级重试测试）
    install_fails: u32,
    recovered: bool,
    failed_with: Option<String>,
}

impl MockOps {
    fn new(scripted: Scripted) -> Self {
        Self {
            calls: Vec::new(),
            scripted,
            agents: Vec::new(),
            install_fails: 0,
            recovered: false,
            failed_with: None,
        }
    }

    fn count(&self, call: &str) -> usize {
        self.calls.iter().filter(|c| **c == call).count()
    }
}

impl ReconnectOps<u32> for MockOps {
    fn stop_render_threads(&mut self) {
        self.calls.push("stop");
    }

    fn spawn_device_recreate(&mut self) -> mpsc::Receiver<Result<(GpuEpoch, u32), String>> {
        self.calls.push("spawn");
        let (tx, rx) = mpsc::channel();
        if let Some(result) = self.scripted.pop_front() {
            let _ = tx.send(result);
        }
        self.agents.push(tx);
        rx
    }

    fn install_and_rebuild_infra(&mut self, _epoch: GpuEpoch, _gpu: u32) -> Result<(), String> {
        self.calls.push("install");
        if self.install_fails > 0 {
            self.install_fails -= 1;
            return Err("安装失败（脚本注入）".to_string());
        }
        Ok(())
    }

    fn mark_replay(&mut self) -> Result<(), String> {
        self.calls.push("replay");
        Ok(())
    }

    fn rebuild_ui(&mut self) -> Result<(), String> {
        self.calls.push("Legacy");
        Ok(())
    }

    fn restart_render_threads(&mut self) -> Result<(), String> {
        self.calls.push("restart");
        Ok(())
    }

    fn on_recovered(&mut self) {
        self.calls.push("recovered");
        self.recovered = true;
    }

    fn on_failed(&mut self, reason: &str) {
        self.calls.push("failed");
        self.failed_with = Some(reason.to_string());
    }
}

/// 驱动会话到终态（步数上限防御死循环）。
fn drive_to_end(session: &mut ReconnectSession<u32>, ops: &mut MockOps) -> bool {
    for _ in 0..64 {
        if session.step(ops) {
            return true;
        }
    }
    false
}

/// 完整成功路径：全步骤按序执行一次，最终 Recovered。
#[test]
fn test_reconnect_full_success_path() {
    let result = Ok((GpuEpoch::INITIAL.next(), 7u32));
    let mut ops = MockOps::new(VecDeque::from([result]));
    let mut session = ReconnectSession::new();
    session.retry_interval = Duration::ZERO;

    assert!(
        drive_to_end(&mut session, &mut ops),
        "应在步数上限内进入终态"
    );
    assert!(matches!(session.state(), ReconnectState::Recovered));
    assert_eq!(
        ops.calls,
        [
            "stop",
            "spawn",
            "install",
            "replay",
            "Legacy",
            "restart",
            "recovered"
        ],
        "全迁移路径的调用序列必须精确匹配"
    );
    assert!(ops.recovered);
    assert!(ops.failed_with.is_none());
}

/// 设备重建连续失败：预算耗尽 → Failed，且 on_failed 恰调用一次。
#[test]
fn test_reconnect_retry_exhaustion_to_failed() {
    let mut scripted = Scripted::new();
    for i in 0..MAX_RECONNECT_ATTEMPTS {
        scripted.push_back(Err(format!("设备失败#{i}")));
    }
    let mut ops = MockOps::new(scripted);
    let mut session = ReconnectSession::new();
    session.retry_interval = Duration::ZERO;

    assert!(drive_to_end(&mut session, &mut ops));
    assert!(matches!(session.state(), ReconnectState::Failed { .. }));
    assert_eq!(ops.count("spawn"), MAX_RECONNECT_ATTEMPTS as usize);
    assert_eq!(ops.count("install"), 0, "设备从未成功，不应进入安装步");
    assert_eq!(ops.count("failed"), 1, "on_failed 必须恰调用一次");
    assert!(
        ops.failed_with
            .as_deref()
            .is_some_and(|r| r.contains("3 次尝试均失败"))
    );
    assert!(!ops.recovered);
}

/// 基础设施步失败：整轮降级重试，第二轮成功恢复（"其余各步失败降级重试"）。
#[test]
fn test_reconnect_infra_failure_degrades_and_recovers() {
    let scripted = Scripted::from([Ok((GpuEpoch(1), 1u32)), Ok((GpuEpoch(2), 2u32))]);
    let mut ops = MockOps::new(scripted);
    ops.install_fails = 1;
    let mut session = ReconnectSession::new();
    session.retry_interval = Duration::ZERO;

    assert!(drive_to_end(&mut session, &mut ops));
    assert!(matches!(session.state(), ReconnectState::Recovered));
    assert_eq!(ops.count("spawn"), 2, "第一轮失败后应发起第二轮设备重建");
    assert_eq!(ops.count("install"), 2, "install 重试一次后成功");
    assert!(ops.recovered);
    assert!(ops.failed_with.is_none());
}

/// 等待 worker 期间步进幂等：不重复发起重建（S2）。
#[test]
fn test_reconnect_step_idempotent_while_waiting() {
    // 不预置结果：worker 永远 pending（sender 保活）
    let mut ops = MockOps::new(Scripted::new());
    let mut session = ReconnectSession::new();
    session.retry_interval = Duration::ZERO;

    for _ in 0..5 {
        assert!(!session.step(&mut ops), "worker 未返回前不得进入终态");
    }
    assert_eq!(ops.count("spawn"), 1, "等待期间重建只发起一次（幂等）");
    assert!(matches!(
        session.state(),
        ReconnectState::RecreateDevice { attempt: 1 }
    ));
    assert!(!session.is_finished());
}

/// RCA 回归：终态重入不得被 `mem::replace` 占位值覆写（Recovered）。
#[test]
fn test_reconnect_recovered_state_survives_step_reentry() {
    let mut ops = MockOps::new(Scripted::new());
    let mut session = ReconnectSession::new();
    session.state = ReconnectState::Recovered;
    let calls_before = ops.calls.len();

    assert!(session.step(&mut ops), "终态重入仍应报告已终结");
    assert!(
        matches!(session.state(), ReconnectState::Recovered),
        "Recovered 终态不得被占位值覆写（实际：{:?}）",
        session.state()
    );
    assert_eq!(
        ops.calls.len(),
        calls_before,
        "终态重入不得产生任何操作副作用"
    );
}

/// RCA 回归：Failed 终态重入不得被覆写，且失败原因保留。
#[test]
fn test_reconnect_failed_state_survives_step_reentry() {
    let mut ops = MockOps::new(Scripted::new());
    let mut session = ReconnectSession::new();
    session.state = ReconnectState::Failed {
        reason: "测试终态".to_string(),
    };
    let calls_before = ops.calls.len();

    assert!(session.step(&mut ops), "终态重入仍应报告已终结");
    assert!(
        matches!(session.state(), ReconnectState::Failed { reason } if reason == "测试终态"),
        "Failed 终态及其 reason 不得被占位值覆写（实际：{:?}）",
        session.state()
    );
    assert_eq!(
        ops.calls.len(),
        calls_before,
        "终态重入不得产生任何操作副作用"
    );
}

/// 蓝军兜底：占位态若被误当作现状读入，必须强制进入 Failed 安全终态，禁止静默推进。
#[test]
fn test_reconnect_transitioning_placeholder_forces_failed_terminal() {
    let mut ops = MockOps::new(Scripted::new());
    let mut session = ReconnectSession::new();
    session.state = ReconnectState::Transitioning;

    assert!(session.step(&mut ops), "占位态必须收敛到终态");
    assert!(
        matches!(session.state(), ReconnectState::Failed { .. }),
        "非法占位态必须强制进入 Failed 安全终态（实际：{:?}）",
        session.state()
    );
    assert!(
        ops.failed_with.is_some(),
        "强制 Failed 必须调用 on_failed（有声失败）"
    );
    assert_eq!(ops.calls.len(), 1, "只应有 on_failed 一次副作用");
}

/// 蓝军兜底：RebuildInfra 阶段若重建产物丢失（会话中途被打断），必须降级重试而非崩溃。
///
/// 对应卡上 P3「恢复中途二次 TDR」在状态机层的保障：Runner 层对重复 TDR 是
/// 「忽略并保持单会话」，状态机层则保证被打断后仍沿合法路径推进。
#[test]
fn test_reconnect_rebuild_infra_missing_pending_degrades_to_retry() {
    let mut ops = MockOps::new(Scripted::new());
    let mut session = ReconnectSession::new();
    session.state = ReconnectState::RebuildInfra;
    session.pending = None; // 模拟状态被打断导致产物丢失
    session.attempt = 1;

    assert!(
        !session.step(&mut ops),
        "单次整轮失败应进入退避重试，而非直接终态"
    );
    assert!(
        matches!(
            session.state(),
            ReconnectState::RecreateDevice { attempt: 2 }
        ),
        "必须降级到下一轮重建（实际：{:?}）",
        session.state()
    );
    assert_eq!(ops.count("install"), 0, "无产物时不得调用安装");
    assert!(session.retry_not_before.is_some(), "失败后应设置退避窗口");
}
