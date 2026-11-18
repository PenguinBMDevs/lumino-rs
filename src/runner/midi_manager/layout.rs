//! 实时端口布局状态机与后台重建（DEBT-05 #122）
//!
//! 布局重建（XSynth 204–288ms / LGS 更重）原先在 UI 线程同步执行，端口编辑
//! 连续点按会反复卡顿。这里提供：
//!
//! - **纯决策函数**（不依赖 API/设备，可单测）：一次布局请求该"立即排程后台"、
//!   "合并到在途任务"还是"暂存等待后端就绪"；
//! - **一次性后台 worker**：150ms 防抖后读取"最新期望值"执行重建，完成后把
//!   API 归还并经通道回传；UI 每帧 `poll_layout_apply` 收尾，全程不阻塞事件循环。
//!
//! 同步路径（装载/关闭/导出/重初始化）在进入前调用 `drain_layout_apply(...)`
//! 等待在途任务归还 API，保证"apply → create_additional_output"的顺序不变。

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::panic_guard::{Guarded, run_guarded};
use super::*;

/// 端口编辑的防抖窗口：停手 150ms 后才真正重建。
pub(super) const LAYOUT_DEBOUNCE: Duration = Duration::from_millis(150);

/// 同步路径等待在途后台重建的上限。
pub(super) const LAYOUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// 一次布局请求的处置决策（纯函数，DEBT-05 #122）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LayoutDecision {
    /// 软件合成后端已就绪且无在途重建：启动后台防抖重建
    Start,
    /// 已有在途重建：仅更新期望值（合并，worker 会读取最新值）
    Coalesce,
    /// 后端未就绪（System 回退 / 异步初始化中）：暂存等待完成对齐
    Stash,
}

/// 决策一次布局请求的处置方式。
pub(super) fn decide_layout_request(
    backend: SynthBackend,
    has_api: bool,
    worker_busy: bool,
) -> LayoutDecision {
    let software_ready = matches!(
        backend,
        SynthBackend::XSynth | SynthBackend::Lgs | SynthBackend::Maestro
    );
    if !software_ready {
        return LayoutDecision::Stash;
    }
    if worker_busy {
        return LayoutDecision::Coalesce;
    }
    if has_api {
        LayoutDecision::Start
    } else {
        LayoutDecision::Stash
    }
}

/// 异步初始化完成后是否需要对齐布局（desired 与 spawned 不一致）。
pub(super) fn should_align_after_init(desired: u8, spawned: u8) -> bool {
    desired != spawned
}

/// 后台重建完成后是否需要"追赶"一次（防抖窗口内期望值又变了）。
pub(super) fn layout_needs_catch_up(desired: u8, applied: u8) -> bool {
    desired != applied
}

/// 后台布局重建在途状态。
pub(super) struct LayoutApplyInFlight {
    /// worker 回传通道
    rx: mpsc::Receiver<LayoutApplyDone>,
    /// 排程时刻（用于总耗时日志）
    started: Instant,
    /// 同步 drain 已超时一次：后续 drain 不再阻塞，交给 `poll_layout_apply` 回收。
    timed_out: bool,
}

/// worker 回传：归还 API + 应用结果（成功时带实际生效的 max_port）。
pub(super) struct LayoutApplyDone {
    /// 归还的 API 实例
    api: MidiApi,
    /// 应用结果
    result: Result<u8, LayoutApplyFailure>,
}

/// 布局重建失败的语义：Err 与 panic 都保持旧布局，但 panic 额外触发后端重初始化。
#[derive(Debug)]
pub(super) enum LayoutApplyFailure {
    /// `set_midi_port_layout` 正常返回 Err（旧管线状态由后端保证）
    Error(String),
    /// worker 内 panic，已由守卫截断并归还 API
    Panicked(String),
}

/// 防抖后读取最新期望值并应用；panic 由守卫截断，API 原样归还。
fn run_layout_apply_worker(
    api: MidiApi,
    desired: Arc<AtomicU8>,
    debounce: Duration,
) -> (MidiApi, Result<u8, LayoutApplyFailure>) {
    let (api, outcome) = run_guarded(api, |api| {
        std::thread::sleep(debounce);
        let target = desired.load(Ordering::SeqCst);
        api.set_midi_port_layout(target).map(|()| target)
    });
    let result = match outcome {
        Guarded::Done(result) => result.map_err(LayoutApplyFailure::Error),
        Guarded::Panicked(message) => Err(LayoutApplyFailure::Panicked(message)),
    };
    (api, result)
}

impl MidiManager {
    /// 端口编辑热路径：后台防抖 + 合并应用（DEBT-05 #122）。
    ///
    /// 立即更新 desired 与共享原子值；worker 在防抖窗口结束时读取最新值，
    /// 连续点按只会落一次重建，且 UI 线程不做重活。
    pub fn apply_midi_port_layout_deferred(&mut self, max_port: u8) {
        self.desired_midi_max_port = max_port;
        self.layout_desired.store(max_port, Ordering::SeqCst);
        match decide_layout_request(
            self.active_backend,
            self.api.is_some(),
            self.layout_apply.is_some(),
        ) {
            LayoutDecision::Start => self.start_layout_apply(),
            LayoutDecision::Coalesce => {
                tracing::info!("MIDI(后台): 布局重建进行中，合并请求 max_port={max_port}")
            }
            LayoutDecision::Stash => tracing::info!(
                "MIDI: 暂存端口布局 max_port={max_port}（等待软件合成后端就绪后应用）"
            ),
        }
    }

    /// 启动一次性后台重建：防抖结束后取最新期望值执行，完成后归还 API。
    fn start_layout_apply(&mut self) {
        let Some(api) = self.api.take() else {
            return;
        };
        let desired = Arc::clone(&self.layout_desired);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (api, result) = run_layout_apply_worker(api, desired, LAYOUT_DEBOUNCE);
            // 先归还所有权：send 之后 API 已离开 worker，后续收尾都在主线程。
            let _ = tx.send(LayoutApplyDone { api, result });
        });
        self.layout_apply = Some(LayoutApplyInFlight {
            rx,
            started: Instant::now(),
            timed_out: false,
        });
        tracing::info!(
            "MIDI(后台): 端口布局重建已排程（{:?} 防抖后执行）",
            LAYOUT_DEBOUNCE
        );
    }

    /// 每帧轮询后台重建结果（由 `handle_midi_reinit` 调用）。
    pub fn poll_layout_apply(&mut self) {
        let Some(inflight) = self.layout_apply.take() else {
            return;
        };
        match inflight.rx.try_recv() {
            Ok(done) => self.finish_layout_apply(done, inflight.started),
            Err(mpsc::TryRecvError::Empty) => {
                self.layout_apply = Some(inflight);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::error!(
                    "MIDI(后台): 端口布局重建线程异常断开（API 已丢失），触发后端重初始化"
                );
                // 兜底：worker 内守卫已覆盖 set_midi_port_layout；能走到这里说明线程
                // 在通道回传前异常终止。置位后同帧 `reinit_if_needed` 会重建后端。
                self.needs_reinit = true;
            }
        }
    }

    /// 同步路径进入前等待在途后台重建收尾（归还 API，至多几轮追赶）。
    ///
    /// `abandon_on_timeout = true`（整后端重初始化）：超时或追赶超限时放弃在途，
    /// 避免迟到的旧 API 覆盖新后端。
    /// `abandon_on_timeout = false`（播放重连 / 同步布局应用）：保留在途，worker
    /// 若最终归还，下一帧 `poll_layout_apply` 仍可回收 API。
    pub(super) fn drain_layout_apply(&mut self, abandon_on_timeout: bool) {
        self.drain_layout_apply_with_timeout(abandon_on_timeout, LAYOUT_DRAIN_TIMEOUT);
    }

    fn drain_layout_apply_with_timeout(&mut self, abandon_on_timeout: bool, timeout: Duration) {
        let mut rounds = 0usize;
        while let Some(mut inflight) = self.layout_apply.take() {
            if inflight.timed_out {
                // 已超时一次：重初始化路径放弃，其它路径留给 poll，避免重复 5s 阻塞。
                if !abandon_on_timeout {
                    self.layout_apply = Some(inflight);
                }
                break;
            }
            rounds += 1;
            if rounds > 4 {
                tracing::error!("MIDI(后台): 布局重建追赶超过 4 轮，放弃同步等待");
                if !abandon_on_timeout {
                    self.layout_apply = Some(inflight);
                }
                break;
            }
            match inflight.rx.recv_timeout(timeout) {
                Ok(done) => self.finish_layout_apply(done, inflight.started),
                Err(e) => {
                    tracing::error!(
                        "MIDI(后台): 等待布局重建收尾失败（{e}），API 暂缺，需重初始化/重启恢复"
                    );
                    if !abandon_on_timeout {
                        // 非重初始化超时：标记后保留在途；后续 drain 不再阻塞，
                        // worker 若最终归还，后续 poll 仍可回收 API。
                        inflight.timed_out = true;
                        self.layout_apply = Some(inflight);
                    }
                    break;
                }
            }
        }
    }

    /// 收尾一次后台重建：归还 API、更新 spawned、必要时追赶最新期望值。
    fn finish_layout_apply(&mut self, done: LayoutApplyDone, started: Instant) {
        self.api = Some(done.api);
        match done.result {
            Ok(applied) => {
                self.spawned_midi_max_port = applied;
                tracing::info!(
                    "MIDI(后台): 端口布局应用完成 max_port={applied}（自排程起 {:?}）",
                    started.elapsed()
                );
                if layout_needs_catch_up(self.desired_midi_max_port, applied) {
                    tracing::info!(
                        "MIDI(后台): 防抖期间期望值已更新，追赶到 max_port={}",
                        self.desired_midi_max_port
                    );
                    self.start_layout_apply();
                }
            }
            Err(failure) => {
                // 失败保持旧布局：不改 spawned，desired 保留待下次事件触发。
                match &failure {
                    LayoutApplyFailure::Error(e) => {
                        tracing::error!("MIDI(后台): 应用端口布局失败（保持旧布局）: {e}");
                    }
                    LayoutApplyFailure::Panicked(msg) => {
                        tracing::error!(
                            "MIDI(后台): 应用端口布局 panic（API 已归还，触发后端重初始化）: {msg}"
                        );
                        self.needs_reinit = true;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
