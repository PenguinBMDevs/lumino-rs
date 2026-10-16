//! REND-019 PR-C：GPU 重连状态机（§1.4）。
//!
//! 纯逻辑：`ReconnectSession::step` 每帧最多推进一步（S1——device 请求可达数百 ms，
//! 禁止同步阻塞 UI 线程）；所有操作经 [`ReconnectOps`] 注入，单测用 Mock 驱动
//! 全部迁移路径（含重试耗尽 → Failed）。每步幂等可重入（S2：winit 可能随时
//! 因窗口关闭打断）。
//!
//! 泛型 `D` = 重建产物中的 device 载荷（生产为 `lumino_gfx::SharedGpu`；测试用
//! 轻量占位类型，避免单测需要真实 GPU）。

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lumino_gfx::GpuEpoch;

/// 最大重连尝试次数（每轮含一次设备重建；超限 → Failed 兜底态）。
///
/// 计划 §1.4：「Failed 仅 RecreateDevice 尝试 MAX_RECONNECT_ATTEMPTS 次仍失败
/// 可达；其余各步失败降级重试」。本实现以"一轮完整流水线"计为一次尝试：
/// 其余步骤失败同样以整轮流水线降级重试兜底（退避后重来，有限次），
/// 最终预算仍以设备重建轮次为准。
pub const MAX_RECONNECT_ATTEMPTS: u8 = 3;

/// 两次尝试之间的退避间隔。
const RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// 重连状态（§1.4）。
#[derive(Debug)]
pub enum ReconnectState {
    /// 收到 lost 事件：停止渲染线程（deadline 用于记录异常滞留）
    StopRenderThread { deadline: Instant },
    /// 重建 adapter/device（异步 worker：本帧发起、后续帧收割）
    RecreateDevice { attempt: u8 },
    /// 重建基础设施：安装新共享状态 + 全部存活窗口重绑
    RebuildInfra,
    /// 数据回放：洋葱皮 force_full_next + 内容脏
    ReplayData,
    /// iced Engine/Renderer 重建（现并入窗口重绑；保留结构位）
    RebuildIced,
    /// 撤销停帧标志 + 重启渲染线程 + 恢复 redraw 调度
    RestartRenderThread,
    /// 终态：恢复常规
    Recovered,
    /// 终态：重试预算耗尽。进入前必须完成自动保存并弹兜底提示（红线 9）
    Failed { reason: String },
}

/// 重连操作面（S3）：由根 crate 的 Runner 实现；测试注入 Mock（泛型 `D`）。
pub trait ReconnectOps<D> {
    /// 停止全部渲染线程（幂等）
    fn stop_render_threads(&mut self);
    /// 发起设备重建（专用线程；返回结果通道——调用方后续帧非阻塞收割）
    fn spawn_device_recreate(&mut self) -> mpsc::Receiver<Result<(GpuEpoch, D), String>>;
    /// 安装新共享状态并重建全部窗口基础设施（幂等）
    fn install_and_rebuild_infra(&mut self, epoch: GpuEpoch, gpu: D) -> Result<(), String>;
    /// 数据回放标记
    fn mark_replay(&mut self) -> Result<(), String>;
    /// iced 重建（保留步骤位；现并入窗口重绑）
    fn rebuild_iced(&mut self) -> Result<(), String>;
    /// 撤销停帧标志 + 重启渲染线程
    fn restart_render_threads(&mut self) -> Result<(), String>;
    /// 终态：恢复成功收尾（状态条撤销等）
    fn on_recovered(&mut self);
    /// 终态：失败兜底（实现方必须先完成自动保存——红线 9）
    fn on_failed(&mut self, reason: &str);
}

/// 重连会话（UI 主循环持有，逐帧 [`step`](Self::step)）。
pub struct ReconnectSession<D> {
    state: ReconnectState,
    /// 设备重建 worker 结果通道（RecreateDevice 阶段持有）
    worker: Option<mpsc::Receiver<Result<(GpuEpoch, D), String>>>,
    /// 重建产物（RecreateDevice → RebuildInfra 之间的暂存）
    pending: Option<(GpuEpoch, D)>,
    /// 下一次允许推进的时刻（重试退避）
    retry_not_before: Option<Instant>,
    /// 退避间隔（测试可缩短）
    retry_interval: Duration,
    /// 当前尝试序号（与 `RecreateDevice { attempt }` 同步，失败路径读取）
    attempt: u8,
}

impl<D> Default for ReconnectSession<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D> ReconnectSession<D> {
    /// 新建会话（从 `StopRenderThread` 起步）。
    pub fn new() -> Self {
        Self {
            state: ReconnectState::StopRenderThread {
                deadline: Instant::now() + Duration::from_secs(2),
            },
            worker: None,
            pending: None,
            retry_not_before: None,
            retry_interval: RETRY_INTERVAL,
            attempt: 0,
        }
    }

    /// 是否已进入终态（Recovered / Failed）。
    pub fn is_finished(&self) -> bool {
        matches!(
            self.state,
            ReconnectState::Recovered | ReconnectState::Failed { .. }
        )
    }

    /// 当前状态（测试断言用；生产日志走 `outcome_label`）。
    #[cfg(test)]
    pub fn state(&self) -> &ReconnectState {
        &self.state
    }

    /// 终态标签（会话收尾日志用）。
    pub fn outcome_label(&self) -> String {
        match &self.state {
            ReconnectState::Recovered => "已恢复（Recovered）".to_string(),
            ReconnectState::Failed { reason } => format!("失败兜底（Failed：{reason}）"),
            other => format!("会话未终结即取出（{other:?}）"),
        }
    }

    /// 每帧最多推进一步（S1）。返回 `true` = 已进入终态。
    ///
    /// 幂等可重入（S2）：worker 发起以 `worker.is_none()` 守卫；各执行步以状态
    /// 迁移为准只执行一次；重入只会在当前状态上重复"等待"而非重复副作用。
    pub fn step(&mut self, ops: &mut impl ReconnectOps<D>) -> bool {
        // 重试退避：窗口内不推进
        if let Some(not_before) = self.retry_not_before
            && Instant::now() < not_before
        {
            return false;
        }
        self.retry_not_before = None;

        // 取出现状推进（占位立即被各臂覆盖；避免与 `&mut self.state` 借用冲突）
        let state = std::mem::replace(&mut self.state, ReconnectState::RebuildIced);
        match state {
            ReconnectState::StopRenderThread { deadline } => {
                if Instant::now() > deadline {
                    tracing::warn!(
                        "重连：停止渲染线程在 deadline 之后才推进（会话被延迟，仍继续）"
                    );
                }
                ops.stop_render_threads();
                self.attempt = 1;
                self.state = ReconnectState::RecreateDevice { attempt: 1 };
                false
            }
            ReconnectState::RecreateDevice { attempt } => {
                self.attempt = attempt;
                if self.worker.is_none() {
                    // 本帧发起（异步；S1：不阻塞 UI）
                    self.worker = Some(ops.spawn_device_recreate());
                    self.state = ReconnectState::RecreateDevice { attempt };
                    return false;
                }
                match self.worker.as_ref().map(|rx| rx.try_recv()) {
                    Some(Ok(Ok((epoch, gpu)))) => {
                        self.worker = None;
                        self.pending = Some((epoch, gpu));
                        self.state = ReconnectState::RebuildInfra;
                        false
                    }
                    Some(Ok(Err(e))) => {
                        self.worker = None;
                        self.pipeline_attempt_failed(ops, e)
                    }
                    Some(Err(mpsc::TryRecvError::Empty)) => {
                        // 仍在重建：保持等待（下帧再收割）
                        self.state = ReconnectState::RecreateDevice { attempt };
                        false
                    }
                    Some(Err(mpsc::TryRecvError::Disconnected)) => {
                        self.worker = None;
                        self.pipeline_attempt_failed(ops, "重建 worker 异常退出".to_string())
                    }
                    None => {
                        self.state = ReconnectState::RecreateDevice { attempt };
                        false
                    }
                }
            }
            ReconnectState::RebuildInfra => {
                let Some((epoch, gpu)) = self.pending.take() else {
                    // 不可达防护：无产物（状态被打断）→ 计一次整轮失败
                    return self
                        .pipeline_attempt_failed(ops, "缺少重建产物（状态中断）".to_string());
                };
                match ops.install_and_rebuild_infra(epoch, gpu) {
                    Ok(()) => {
                        self.state = ReconnectState::ReplayData;
                        false
                    }
                    Err(e) => self.pipeline_attempt_failed(ops, e),
                }
            }
            ReconnectState::ReplayData => match ops.mark_replay() {
                Ok(()) => {
                    self.state = ReconnectState::RebuildIced;
                    false
                }
                Err(e) => self.pipeline_attempt_failed(ops, e),
            },
            ReconnectState::RebuildIced => match ops.rebuild_iced() {
                Ok(()) => {
                    self.state = ReconnectState::RestartRenderThread;
                    false
                }
                Err(e) => self.pipeline_attempt_failed(ops, e),
            },
            ReconnectState::RestartRenderThread => match ops.restart_render_threads() {
                Ok(()) => {
                    ops.on_recovered();
                    self.state = ReconnectState::Recovered;
                    true
                }
                Err(e) => self.pipeline_attempt_failed(ops, e),
            },
            ReconnectState::Recovered | ReconnectState::Failed { .. } => true,
        }
    }

    /// 整轮流水线失败：预算内退避重试，耗尽进 Failed（`on_failed` 保证先自动保存）。
    fn pipeline_attempt_failed(&mut self, ops: &mut impl ReconnectOps<D>, err: String) -> bool {
        let attempt = self.attempt;
        if attempt >= MAX_RECONNECT_ATTEMPTS {
            let reason = format!("重连流水线 {attempt} 次尝试均失败：{err}");
            tracing::error!("{reason}");
            ops.on_failed(&reason);
            self.state = ReconnectState::Failed { reason };
            return true;
        }
        tracing::warn!("重连尝试 {attempt}/{MAX_RECONNECT_ATTEMPTS} 失败：{err}（将重试）");
        self.retry_not_before = Some(Instant::now() + self.retry_interval);
        self.attempt = attempt + 1;
        self.state = ReconnectState::RecreateDevice {
            attempt: self.attempt,
        };
        false
    }
}

/// R0：状态机全迁移路径测试（Mock ReconnectOps，无 GPU 依赖）。
#[cfg(test)]
#[path = "reconnect/tests.rs"]
mod tests;
