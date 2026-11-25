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
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use lumino_midi_model::multi_port::PortSoundfontMap;

use super::panic_guard::{Guarded, run_guarded};
use super::*;

mod state;

pub(super) use state::{SynthAppliedState, SynthApplyOutcome, synth_outcome_is_stale};

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

/// 异步初始化完成后是否需要对齐端口音色库（desired 与 spawned 不一致）。
pub(super) fn soundfonts_need_align(
    desired: &PortSoundfontMap,
    spawned: &PortSoundfontMap,
) -> bool {
    desired != spawned
}

/// 异步初始化完成后是否需要对齐合成状态（布局或端口音色库）。
pub(super) fn synth_needs_alignment(
    desired_layout: u8,
    spawned_layout: u8,
    desired_soundfonts: &PortSoundfontMap,
    spawned_soundfonts: &PortSoundfontMap,
) -> bool {
    should_align_after_init(desired_layout, spawned_layout)
        || soundfonts_need_align(desired_soundfonts, spawned_soundfonts)
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

/// worker 回传：归还 API + 应用结果。
pub(super) struct LayoutApplyDone {
    /// 归还的 API 实例
    api: MidiApi,
    /// 应用结果
    result: Result<SynthApplyOutcome, LayoutApplyFailure>,
    /// 排程时的后端代际；与 `MidiManager::synth_epoch` 不一致则结果作废。
    epoch: u64,
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
///
/// 顺序契约（REND-005）：
/// 1. **仅在布局变化时**调用 `set_midi_port_layout`（同布局的轻量复位会
///    `SystemReset` + 杀音，改映射时绝不能走）；
/// 2. 映射变化且当前不在播放时调用 `apply_port_soundfonts`（只换矩阵、不杀音）；
/// 3. 播放中到达的映射请求原样保留，由 `tick_port_soundfonts` 在停播后再应用。
fn run_synth_apply_worker(
    api: MidiApi,
    desired: Arc<AtomicU8>,
    desired_soundfonts: Arc<Mutex<PortSoundfontMap>>,
    applied: SynthAppliedState,
    playback_active: Arc<AtomicBool>,
    debounce: Duration,
) -> (MidiApi, Result<SynthApplyOutcome, LayoutApplyFailure>) {
    let (api, outcome) = run_guarded(api, |api| {
        std::thread::sleep(debounce);
        let target_layout = desired.load(Ordering::SeqCst);
        let target_soundfonts = desired_soundfonts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        let mut applied = applied;

        if target_layout != applied.max_port {
            api.set_midi_port_layout(target_layout)
                .map_err(LayoutApplyFailure::Error)?;
            applied.max_port = target_layout;
        }

        let mut notice = None;
        if target_soundfonts != applied.port_soundfonts && !playback_active.load(Ordering::SeqCst) {
            match api.apply_port_soundfonts(&target_soundfonts) {
                Ok(()) => applied.port_soundfonts = target_soundfonts,
                Err(message) => {
                    // 后端已按失败端口回退全局；记录已应用避免每帧重试，
                    // 把原因交给状态栏提示。用户重新提交映射即可再次尝试。
                    applied.port_soundfonts = target_soundfonts;
                    notice = Some(message);
                }
            }
        }

        Ok(SynthApplyOutcome { applied, notice })
    });
    match outcome {
        Guarded::Done(result) => (api, result),
        Guarded::Panicked(message) => (api, Err(LayoutApplyFailure::Panicked(message))),
    }
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
    ///
    /// REND-005：同一次 worker 同时落定「布局 + 端口音色库」，避免两条链路
    /// 争抢 API 所有权；布局未变时不会调用 `set_midi_port_layout`。
    pub(super) fn start_layout_apply(&mut self) {
        let Some(api) = self.api.take() else {
            return;
        };
        let desired = Arc::clone(&self.layout_desired);
        let desired_soundfonts = Arc::clone(&self.port_soundfonts_desired);
        let playback_active = Arc::clone(&self.playback_active);
        let epoch = self.synth_epoch;
        let applied =
            Self::applied_synth_state(self.spawned_midi_max_port, &self.spawned_port_soundfonts);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (api, result) = run_synth_apply_worker(
                api,
                desired,
                desired_soundfonts,
                applied,
                playback_active,
                LAYOUT_DEBOUNCE,
            );
            // 先归还所有权：send 之后 API 已离开 worker，后续收尾都在主线程。
            let _ = tx.send(LayoutApplyDone { api, result, epoch });
        });
        self.layout_apply = Some(LayoutApplyInFlight {
            rx,
            started: Instant::now(),
            timed_out: false,
        });
        tracing::info!(
            "MIDI(后台): 合成状态应用已排程（{:?} 防抖后执行）",
            LAYOUT_DEBOUNCE
        );
    }

    /// 构造“已应用状态”快照（供 worker 判断是否需要重建/换库）。
    fn applied_synth_state(max_port: u8, port_soundfonts: &PortSoundfontMap) -> SynthAppliedState {
        SynthAppliedState {
            max_port,
            port_soundfonts: port_soundfonts.clone(),
        }
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

    /// 收尾一次后台重建：归还 API、更新已应用状态、必要时追赶最新期望值。
    fn finish_layout_apply(&mut self, done: LayoutApplyDone, started: Instant) {
        // REND-005：旧后端代际的结果一律丢弃——包括“当前后端不支持按端口音色库”
        // 这类能力提示；切到 XSynth 后旧 LGS worker 的提示不能污染状态栏。
        // API 也不回收：调用方（reinit）随后会丢弃/替换整个后端实例。
        if synth_outcome_is_stale(done.epoch, self.synth_epoch) {
            tracing::info!(
                "MIDI(后台): 丢弃旧后端代的合成状态结果（epoch {} -> {}）",
                done.epoch,
                self.synth_epoch
            );
            return;
        }
        self.api = Some(done.api);
        match done.result {
            Ok(outcome) => {
                self.spawned_midi_max_port = outcome.applied.max_port;
                self.spawned_port_soundfonts = outcome.applied.port_soundfonts;
                tracing::info!(
                    "MIDI(后台): 合成状态应用完成 max_port={}（自排程起 {:?}）",
                    self.spawned_midi_max_port,
                    started.elapsed()
                );
                if let Some(notice) = outcome.notice {
                    if self.active_backend == SynthBackend::XSynth {
                        tracing::warn!("MIDI(后台): 端口音色库回退提示: {notice}");
                        self.pending_backend_notice = Some(notice);
                    } else {
                        // 不支持的后端（LGS/Maestro/System）由专用弹窗告知，
                        // 不再用左下角状态栏一闪而过；不重置 ack，避免布局
                        // 重建等无关事件让用户重复确认。
                        tracing::info!(
                            "MIDI(后台): 非 XSynth 后端的端口音色库提示改由弹窗展示: {notice}"
                        );
                    }
                }
                if self.synth_needs_catch_up() {
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

    /// 是否还有未落定的合成状态（布局或停播后的端口音色库）。
    ///
    /// 播放中不追赶音色库：worker 会跳过，追赶只会形成空转循环；停播后由
    /// [`Self::tick_port_soundfonts`] 排程。
    fn synth_needs_catch_up(&self) -> bool {
        let layout_pending =
            layout_needs_catch_up(self.desired_midi_max_port, self.spawned_midi_max_port);
        let soundfonts_pending =
            soundfonts_need_align(&self.desired_port_soundfonts, &self.spawned_port_soundfonts)
                && !self.playback_active.load(Ordering::SeqCst);
        layout_pending || soundfonts_pending
    }
}

#[cfg(test)]
mod tests;
