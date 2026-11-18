//! REND-019 PR-D（§1.5 L2）：LGS 独立 device 的 TDR 重建（api 暂持 worker + 每帧收尾）。
//!
//! 复用 DEBT-05 #122 布局重建（layout.rs）的机制：一次性后台 worker 暂持 API，
//! 调 `SynthControl::rebuild_gpu_pipeline` 热替换管线（先建新、成功才停旧），
//! 完成后归还并经通道回传；UI 每帧 `poll_lgs_rebuild` 收尾，全程不阻塞事件循环。
//!
//! **AUD-008：所有权不因 panic 蒸发**——worker 内 `catch_unwind` 截断 panic，
//! 成功 / Err / panic 三条路径都把同一个 `MidiApi` 经通道归还；主线程统一按
//! 失败语义退避重试。注意 `Lgs::rebuild_with_options` 在 `build_synth` 成功后
//! 已停旧流：因此「句柄保留」只保证 API 对象不丢，音频恢复依赖重试成功。
//!
//! 与编辑器主重连会话**完全独立**（红线 8：LGS 与编辑器零共享 device、互不等待）。
//! 耗尽后一次性降级提示（不静默）。
//!
//! 「断点续播」（§1.5 L2）：播放传输不因本流程中断，重建后事件流经热替换的
//! 发送器自动接续；若 R5 实测发现传输停摆，再补显式 seek 到 `playback_position`。

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lumino_core::tr;

use super::panic_guard::{Guarded, run_guarded};
use super::*;

/// 重建失败后的退避间隔。
pub(super) const LGS_REBUILD_BACKOFF: Duration = Duration::from_secs(1);
/// 最大尝试次数（耗尽 → 一次性降级提示）。
pub(super) const MAX_LGS_REBUILD_ATTEMPTS: u8 = 3;
/// 同步路径等待在途重建归还 API 的上限。
pub(super) const LGS_REBUILD_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// 一次 LGS 重建请求的处置决策（纯函数，可单测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LgsRebuildDecision {
    /// 立即排程后台重建
    Start,
    /// API 被其它 worker（布局重建）暂持：暂存请求，空闲后自动开始
    Defer,
    /// 非 LGS 活跃后端 / API 不可用：不排程
    Reject,
}

/// 决策一次 LGS 重建请求的处置方式。
pub(super) fn decide_lgs_rebuild(
    backend: SynthBackend,
    has_api: bool,
    worker_busy: bool,
) -> LgsRebuildDecision {
    if backend != SynthBackend::Lgs || !has_api {
        return LgsRebuildDecision::Reject;
    }
    if worker_busy {
        return LgsRebuildDecision::Defer;
    }
    LgsRebuildDecision::Start
}

/// 失败后的下一步（纯函数，可单测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LgsRetryStep {
    /// 退避后重试
    Retry,
    /// 次数耗尽 → 降级提示
    Exhausted,
}

/// 依已尝试次数给出下一步。
#[must_use]
pub(super) fn next_retry_step(attempts_done: u8) -> LgsRetryStep {
    if attempts_done >= MAX_LGS_REBUILD_ATTEMPTS {
        LgsRetryStep::Exhausted
    } else {
        LgsRetryStep::Retry
    }
}

/// TDR 重建状态（请求暂存 / 在途 worker / 退避 / 尝试计数）。
#[derive(Default)]
pub(super) struct LgsRebuildState {
    /// 有效请求：含「布局 worker 占用的 Defer 暂存」与「失败退避后的重试」
    requested: bool,
    /// 在途 worker（持有 API）
    inflight: Option<LgsRebuildInFlight>,
    /// 已尝试次数（成功 / 耗尽后归零）
    attempts: u8,
    /// 退避：不早于该时刻再次排程
    not_before: Option<Instant>,
}

pub(super) struct LgsRebuildInFlight {
    rx: mpsc::Receiver<LgsRebuildDone>,
    started: Instant,
}

/// worker 回传：归还 API + 重建结果。
pub(super) struct LgsRebuildDone {
    api: MidiApi,
    result: Result<(), LgsRebuildFailure>,
}

/// 重建失败的语义：Err 与 panic 走同一退避重试，但日志/归因不同。
#[derive(Debug)]
pub(super) enum LgsRebuildFailure {
    /// `rebuild_gpu_pipeline` 正常返回 Err（API 句柄保留，音频流可能已停）。
    Error(String),
    /// worker 内 panic，已由 `catch_unwind` 截断并归还 API。
    Panicked(String),
}

/// 执行一次重建并**无条件**归还 API：panic 由 [`run_guarded`] 截断后同样返回。
///
/// `AssertUnwindSafe` 的成立依据见 [`run_guarded`]：panic 后只把 API 交还主线程
/// 按失败处理，不假设其内部状态仍可正常发声。
fn run_lgs_rebuild_worker(api: MidiApi) -> (MidiApi, Result<(), LgsRebuildFailure>) {
    let (api, outcome) = run_guarded(api, |api| api.rebuild_gpu_pipeline());
    let result = match outcome {
        Guarded::Done(Ok(())) => Ok(()),
        Guarded::Done(Err(e)) => Err(LgsRebuildFailure::Error(e)),
        Guarded::Panicked(message) => Err(LgsRebuildFailure::Panicked(message)),
    };
    (api, result)
}

impl MidiManager {
    /// 受理一次 LGS TDR 重建请求（`UserEvent::LgsDeviceLost` 入口调用）。
    ///
    /// 返回 `true` = 已排程或已暂存（后续由 `poll_lgs_rebuild` 推进）；
    /// `false` = 非 LGS 活跃后端 / API 不可用（事件被忽略）。
    pub fn request_lgs_rebuild(&mut self) -> bool {
        if self.lgs_rebuild.requested || self.lgs_rebuild.inflight.is_some() {
            tracing::info!("LGS（PR-D）: 重建已在进行/已暂存，忽略重复请求");
            return true;
        }
        match decide_lgs_rebuild(
            self.active_backend,
            self.api.is_some(),
            self.layout_apply.is_some(),
        ) {
            LgsRebuildDecision::Start => {
                self.lgs_rebuild.attempts = 0;
                self.start_lgs_rebuild();
                true
            }
            LgsRebuildDecision::Defer => {
                self.lgs_rebuild.requested = true;
                tracing::info!("LGS（PR-D）: 重建暂存（布局重建在途，空闲后自动开始）");
                true
            }
            LgsRebuildDecision::Reject => {
                tracing::warn!("LGS（PR-D）: 非 LGS 活跃后端或 API 不可用，忽略 device-lost");
                false
            }
        }
    }

    /// 每帧推进（`handle_midi_reinit` 调用）：收尾在途 worker + 排程暂存/退避请求。
    pub fn poll_lgs_rebuild(&mut self) {
        if let Some(inflight) = self.lgs_rebuild.inflight.take() {
            match inflight.rx.try_recv() {
                Ok(done) => self.finish_lgs_rebuild(done, inflight.started),
                Err(mpsc::TryRecvError::Empty) => {
                    self.lgs_rebuild.inflight = Some(inflight);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::error!(
                        "LGS（PR-D）: 重建线程异常断开（API 已丢失），触发后端重初始化"
                    );
                    // 兜底：worker 内 catch 已覆盖 rebuild 调用；能走到这里说明线程在
                    // 通道回传前异常终止。置位后本次 `handle_midi_reinit` 的
                    // `reinit_if_needed` 会重建整个后端（LGS 异步初始化/System 回退）。
                    self.needs_reinit = true;
                }
            }
        }
        if self.lgs_rebuild.requested
            && self.lgs_rebuild.inflight.is_none()
            && self.api.is_some()
            && self.layout_apply.is_none()
        {
            let ready = self
                .lgs_rebuild
                .not_before
                .is_none_or(|at| Instant::now() >= at);
            if ready {
                self.lgs_rebuild.not_before = None;
                self.start_lgs_rebuild();
            }
        }
    }

    /// 同步路径进入前等待在途重建归还 API。
    ///
    /// `cancel_retry = true`（整后端重初始化）：取消暂存/退避请求，只归还 API——
    /// 随后的 `reinit_if_needed` 会重建整个后端。
    /// `cancel_retry = false`（播放输出重连 / 同步布局应用）：**保留**待重试；
    /// 在途结果按 `finish_lgs_rebuild` 正常收尾（失败则继续退避重试），避免一次
    /// 用户操作把 panic/Err 后的恢复机会静默取消。
    pub(super) fn drain_lgs_rebuild(&mut self, cancel_retry: bool) {
        if cancel_retry {
            self.lgs_rebuild.requested = false;
            self.lgs_rebuild.not_before = None;
        }
        if let Some(inflight) = self.lgs_rebuild.inflight.take() {
            match inflight.rx.recv_timeout(LGS_REBUILD_DRAIN_TIMEOUT) {
                Ok(done) => {
                    if cancel_retry {
                        self.api = Some(done.api);
                        match &done.result {
                            Ok(()) => {}
                            Err(LgsRebuildFailure::Error(e)) => {
                                tracing::error!(
                                    "LGS（PR-D）: 同步等待期间重建失败（API 已归还）: {e}"
                                );
                            }
                            Err(LgsRebuildFailure::Panicked(msg)) => {
                                tracing::error!(
                                    "LGS（PR-D）: 同步等待期间重建 panic（API 已兜底归还）: {msg}"
                                );
                            }
                        }
                    } else {
                        // 非重初始化路径：按正常收尾推进 attempts/退避/提示，
                        // 不把失败重试静默吞掉。
                        self.finish_lgs_rebuild(done, inflight.started);
                    }
                }
                Err(e) => tracing::error!(
                    "LGS（PR-D）: 等待重建收尾失败（{e}），API 暂缺，需重初始化/重启恢复"
                ),
            }
        }
    }

    fn start_lgs_rebuild(&mut self) {
        let Some(api) = self.api.take() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (api, result) = run_lgs_rebuild_worker(api);
            // 先归还所有权：send 是 worker 的最后一步，API 离开 worker 后，
            // 任何收尾（日志/状态/重试决策）都在主线程，不会把引擎一起带走。
            let _ = tx.send(LgsRebuildDone { api, result });
        });
        self.lgs_rebuild.inflight = Some(LgsRebuildInFlight {
            rx,
            started: Instant::now(),
        });
        tracing::info!("LGS（PR-D）: 重建已排程（后台热替换）");
    }

    fn finish_lgs_rebuild(&mut self, done: LgsRebuildDone, started: Instant) {
        self.api = Some(done.api);
        match done.result {
            Ok(()) => {
                tracing::info!(
                    "LGS（PR-D）: GPU 合成管线已重建（自排程 {:?}），事件流自动接续",
                    started.elapsed()
                );
                self.lgs_rebuild.requested = false;
                self.lgs_rebuild.attempts = 0;
                self.lgs_rebuild.not_before = None;
                self.pending_backend_notice = Some(tr!("GPU 合成已恢复").to_string());
            }
            Err(failure) => {
                self.lgs_rebuild.attempts = self.lgs_rebuild.attempts.saturating_add(1);
                match &failure {
                    LgsRebuildFailure::Error(e) => tracing::error!(
                        "LGS（PR-D）: 重建失败（第 {} 次，API 已保留）: {e}",
                        self.lgs_rebuild.attempts
                    ),
                    LgsRebuildFailure::Panicked(msg) => tracing::error!(
                        "LGS（PR-D）: 重建 panic（第 {} 次，API 已兜底归还）: {msg}",
                        self.lgs_rebuild.attempts
                    ),
                }
                match next_retry_step(self.lgs_rebuild.attempts) {
                    LgsRetryStep::Retry => {
                        self.lgs_rebuild.requested = true;
                        self.lgs_rebuild.not_before = Some(Instant::now() + LGS_REBUILD_BACKOFF);
                    }
                    LgsRetryStep::Exhausted => {
                        self.lgs_rebuild.requested = false;
                        self.lgs_rebuild.not_before = None;
                        self.pending_backend_notice = Some(
                            tr!("GPU 合成不可用（重建多次失败）：音频将保持静音，请重启 Lumino")
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
