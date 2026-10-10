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
//! 同步路径（装载/关闭/导出/重初始化）在进入前调用 `drain_layout_apply()`
//! 等待在途任务归还 API，保证"apply → create_additional_output"的顺序不变。

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
    let software_ready = matches!(backend, SynthBackend::XSynth | SynthBackend::Lgs);
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
}

/// worker 回传：归还 API + 应用结果（成功时带实际生效的 max_port）。
pub(super) struct LayoutApplyDone {
    /// 归还的 API 实例
    api: MidiApi,
    /// 应用结果
    result: Result<u8, String>,
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
        let Some(mut api) = self.api.take() else {
            return;
        };
        let desired = Arc::clone(&self.layout_desired);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            std::thread::sleep(LAYOUT_DEBOUNCE);
            let target = desired.load(Ordering::SeqCst);
            let started = Instant::now();
            let result = api.set_midi_port_layout(target).map(|()| target);
            match &result {
                Ok(applied) => tracing::info!(
                    "MIDI(后台): 端口布局已重建 max_port={applied}，重建耗时 {:?}",
                    started.elapsed()
                ),
                Err(e) => tracing::error!("MIDI(后台): 端口布局重建失败（保持旧布局）: {e}"),
            }
            let _ = tx.send(LayoutApplyDone { api, result });
        });
        self.layout_apply = Some(LayoutApplyInFlight {
            rx,
            started: Instant::now(),
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
                tracing::error!("MIDI(后台): 端口布局重建线程异常断开，等待重初始化恢复");
            }
        }
    }

    /// 同步路径进入前等待在途后台重建收尾（归还 API，至多几轮追赶）。
    pub(super) fn drain_layout_apply(&mut self) {
        let mut rounds = 0usize;
        while let Some(inflight) = self.layout_apply.take() {
            rounds += 1;
            if rounds > 4 {
                tracing::error!("MIDI(后台): 布局重建追赶超过 4 轮，放弃同步等待");
                break;
            }
            match inflight.rx.recv_timeout(LAYOUT_DRAIN_TIMEOUT) {
                Ok(done) => self.finish_layout_apply(done, inflight.started),
                Err(e) => {
                    tracing::error!(
                        "MIDI(后台): 等待布局重建收尾失败（{e}），API 暂缺，将在重初始化恢复"
                    );
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
            Err(e) => {
                // 失败保持旧布局：不改 spawned，desired 保留待下次事件触发。
                tracing::error!("MIDI(后台): 应用端口布局失败（保持旧布局）: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 五条状态路径（DEBT-05 #122 验收）：System 回退暂存 / 初始化中暂存 /
    /// 就绪排程 / 在途合并 / LGS 就绪排程。
    #[test]
    fn decide_layout_request_covers_five_paths() {
        use SynthBackend::{Lgs, System, XSynth};

        // 1) System 回退：暂存等待
        assert_eq!(
            decide_layout_request(System, true, false),
            LayoutDecision::Stash
        );
        // 2) XSynth 异步初始化中（无 api）：暂存
        assert_eq!(
            decide_layout_request(XSynth, false, false),
            LayoutDecision::Stash
        );
        // 3) XSynth 就绪且空闲：启动后台重建
        assert_eq!(
            decide_layout_request(XSynth, true, false),
            LayoutDecision::Start
        );
        // 4) 已有在途重建（api 已被 worker 取走）：合并请求
        assert_eq!(
            decide_layout_request(XSynth, false, true),
            LayoutDecision::Coalesce
        );
        // 5) LGS 就绪且空闲：同样启动后台重建
        assert_eq!(
            decide_layout_request(Lgs, true, false),
            LayoutDecision::Start
        );
    }

    /// 完成对齐：desired == spawned 不需要重建；不同则需要。
    #[test]
    fn should_align_after_init_compares_desired_and_spawned() {
        assert!(!should_align_after_init(7, 7), "一致时不得白重建");
        assert!(should_align_after_init(7, 0), "初始化期间文档切换需对齐");
        assert!(should_align_after_init(0, 3), "关闭归零同样需要对齐");
    }

    /// 追赶：防抖窗口内期望值更新过才追赶，避免重复重建。
    #[test]
    fn layout_needs_catch_up_only_on_change() {
        assert!(!layout_needs_catch_up(5, 5));
        assert!(layout_needs_catch_up(5, 2));
        assert!(layout_needs_catch_up(0, 4), "关闭归零也要追赶到 0");
    }
}
