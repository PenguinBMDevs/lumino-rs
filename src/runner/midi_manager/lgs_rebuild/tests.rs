//! `lgs_rebuild` 单元测试：决策/重试边界 + AUD-008 panic 所有权归还。
//!
//! 拆到独立文件以守住 400 行守卫；`MockApi` 来自 `test_support`，不触碰任何真实驱动。

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lumino_core::storage::config::SynthBackend;
use lumino_core::tr;

use super::super::MidiApi;
use super::super::test_support::MockApi;
use super::*;
use crate::runner::midi_manager::MidiManager;

/// 三条决策路径：非 LGS / API 缺失 → 拒绝；布局 worker 占用 → 暂存；空闲 LGS → 排程。
#[test]
fn decide_lgs_rebuild_covers_three_paths() {
    use SynthBackend::{Lgs, System, XSynth};
    assert_eq!(
        decide_lgs_rebuild(System, true, false),
        LgsRebuildDecision::Reject
    );
    assert_eq!(
        decide_lgs_rebuild(XSynth, true, false),
        LgsRebuildDecision::Reject
    );
    assert_eq!(
        decide_lgs_rebuild(Lgs, false, false),
        LgsRebuildDecision::Reject
    );
    assert_eq!(
        decide_lgs_rebuild(Lgs, true, true),
        LgsRebuildDecision::Defer
    );
    assert_eq!(
        decide_lgs_rebuild(Lgs, true, false),
        LgsRebuildDecision::Start
    );
}

/// 重试边界：MAX-1 次仍重试；MAX 次与溢出值耗尽。
#[test]
fn next_retry_step_boundary() {
    assert_eq!(next_retry_step(0), LgsRetryStep::Retry);
    assert_eq!(
        next_retry_step(MAX_LGS_REBUILD_ATTEMPTS - 1),
        LgsRetryStep::Retry
    );
    assert_eq!(
        next_retry_step(MAX_LGS_REBUILD_ATTEMPTS),
        LgsRetryStep::Exhausted
    );
    assert_eq!(next_retry_step(u8::MAX), LgsRetryStep::Exhausted);
}

fn manager_with(api: MidiApi) -> MidiManager {
    MidiManager {
        api: Some(api),
        active_backend: SynthBackend::Lgs,
        ..MidiManager::default()
    }
}

/// 轮询到 worker 收尾（上限 2s，避免测试挂死）。
fn poll_until_done(manager: &mut MidiManager) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while manager.lgs_rebuild.inflight.is_some() && Instant::now() < deadline {
        manager.poll_lgs_rebuild();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        manager.lgs_rebuild.inflight.is_none(),
        "worker 未在 2s 内收尾"
    );
}

/// AUD-008：worker 内 panic 后，同一个 API 必须原样返回，并携带 panic 语义。
#[test]
fn worker_panic_returns_api_and_panic_failure() {
    let (api, result) =
        run_lgs_rebuild_worker(MockApi::panic_on_rebuild("注入的重建 panic").boxed());
    assert!(api.version().is_some(), "panic 后 API 不得蒸发");

    match result {
        Err(LgsRebuildFailure::Panicked(msg)) => {
            assert!(msg.contains("注入的重建 panic"), "panic 消息应可读: {msg}");
        }
        other => panic!("应回传 Panicked，实际: {other:?}"),
    }
}

/// AUD-008：request → poll 的 panic 路径：API 归还、计一次失败、排程退避重试。
#[test]
fn request_poll_panic_keeps_api_and_schedules_retry() {
    let mut manager = manager_with(MockApi::panic_on_rebuild("注入的重建 panic").boxed());
    assert!(manager.request_lgs_rebuild(), "LGS 活跃且 API 就绪应受理");

    poll_until_done(&mut manager);

    assert!(manager.api.is_some(), "panic 后 API 不得蒸发");
    assert_eq!(manager.lgs_rebuild.attempts, 1, "panic 应计一次失败");
    assert!(manager.lgs_rebuild.requested, "应排程退避重试");
    assert!(manager.lgs_rebuild.not_before.is_some(), "应设置退避时刻");
}

/// 非重初始化路径的 drain（播放重连/同步布局）：panic 后 API 归还且**保留重试**。
#[test]
fn drain_panic_keeps_api_and_schedules_retry() {
    let mut manager = manager_with(MockApi::panic_on_rebuild("注入的重建 panic").boxed());
    assert!(manager.request_lgs_rebuild());

    manager.drain_lgs_rebuild(false);

    assert!(manager.api.is_some(), "drain 路径 panic 后 API 必须归还");
    assert_eq!(manager.lgs_rebuild.attempts, 1, "drain 应按失败收尾");
    assert!(manager.lgs_rebuild.requested, "非重初始化路径不得取消重试");
}

/// 整后端重初始化的 drain：只归还 API、取消待重试（随后 reinit 会重建后端）。
#[test]
fn reinit_drain_cancels_retry() {
    let mut manager = manager_with(MockApi::panic_on_rebuild("注入的重建 panic").boxed());
    assert!(manager.request_lgs_rebuild());

    manager.drain_lgs_rebuild(true);

    assert!(manager.api.is_some(), "drain 路径 panic 后 API 必须归还");
    assert_eq!(manager.lgs_rebuild.attempts, 0, "重初始化路径不计数");
    assert!(!manager.lgs_rebuild.requested, "重初始化路径取消待重试");
}

/// panic 计满尝试次数 → 沿用 #152 的一次性提示，且不再自动重试。
#[test]
fn panic_exhaustion_emits_notice() {
    let mut manager = manager_with(MockApi::ok().boxed());
    manager.lgs_rebuild.attempts = MAX_LGS_REBUILD_ATTEMPTS - 1;

    manager.finish_lgs_rebuild(
        LgsRebuildDone {
            api: MockApi::ok().boxed(),
            result: Err(LgsRebuildFailure::Panicked("最后一次".to_string())),
        },
        Instant::now(),
    );

    assert_eq!(manager.lgs_rebuild.attempts, MAX_LGS_REBUILD_ATTEMPTS);
    assert!(!manager.lgs_rebuild.requested, "耗尽后不再自动重试");
    assert!(manager.api.is_some(), "耗尽时 API 仍应归还");
    assert_eq!(
        manager.take_pending_backend_notice(),
        Some(tr!("GPU 合成不可用（重建多次失败）：音频将保持静音，请重启 Lumino").to_string())
    );
}

/// 正常重建路径零回归：成功清计数、清重试、提示恢复。
#[test]
fn success_path_unchanged() {
    let mut manager = manager_with(MockApi::ok().boxed());
    assert!(manager.request_lgs_rebuild());

    poll_until_done(&mut manager);

    assert!(manager.api.is_some());
    assert_eq!(manager.lgs_rebuild.attempts, 0);
    assert!(!manager.lgs_rebuild.requested);
    assert_eq!(
        manager.take_pending_backend_notice(),
        Some(tr!("GPU 合成已恢复").to_string())
    );
}

/// 通道在回传前断开（守卫未覆盖的进程级异常）：置 needs_reinit，让重初始化兜底。
#[test]
fn disconnected_requests_reinit() {
    let mut manager = MidiManager::default();
    let (tx, rx) = mpsc::channel();
    drop(tx);
    manager.lgs_rebuild.inflight = Some(LgsRebuildInFlight {
        rx,
        started: Instant::now(),
    });

    manager.poll_lgs_rebuild();

    assert!(manager.needs_reinit(), "线程断开应触发后端重初始化兜底");
}
