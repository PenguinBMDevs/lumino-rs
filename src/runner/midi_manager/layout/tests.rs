//! `layout` 单元测试：决策/追赶边界 + 后台 worker 与同步路径的 panic 所有权归还。
//!
//! 使用 `test_support::MockApi`（零驱动）；worker 用例不触碰真实 GPU/音频设备。

use lumino_core::storage::config::SynthBackend;

use super::super::MidiApi;
use super::super::test_support::MockApi;
use super::*;
use crate::runner::midi_manager::MidiManager;

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
    // 6) Maestro 就绪且空闲：同样启动后台重建（AUD-009）
    assert_eq!(
        decide_layout_request(SynthBackend::Maestro, true, false),
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

fn manager_with(api: MidiApi) -> MidiManager {
    MidiManager {
        api: Some(api),
        active_backend: SynthBackend::Lgs,
        ..MidiManager::default()
    }
}

/// 轮询到 worker 收尾（上限 2s，避免测试挂死）。
fn poll_layout_until_done(manager: &mut MidiManager) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while manager.layout_apply.is_some() && Instant::now() < deadline {
        manager.poll_layout_apply();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(manager.layout_apply.is_none(), "worker 未在 2s 内收尾");
}

/// AUD-008 同域：布局 worker 内 panic 后，同一个 API 必须原样返回并转成 Err。
#[test]
fn worker_panic_returns_api_and_error() {
    let desired = Arc::new(AtomicU8::new(3));
    let (api, result) = run_layout_apply_worker(
        MockApi::panic_on_layout("注入的布局 panic").boxed(),
        desired,
        Duration::ZERO,
    );
    assert!(api.version().is_some(), "panic 后 API 不得蒸发");
    match result {
        Err(LayoutApplyFailure::Panicked(msg)) => {
            assert!(msg.contains("注入的布局 panic"), "panic 消息应可读: {msg}");
        }
        other => panic!("应回传 Panicked，实际: {other:?}"),
    }
}

/// 后台防抖路径 panic：API 归还、失败不提交布局。
#[test]
fn deferred_panic_keeps_api() {
    let mut manager = manager_with(MockApi::panic_on_layout("注入的布局 panic").boxed());
    manager.apply_midi_port_layout_deferred(4);

    poll_layout_until_done(&mut manager);

    assert!(manager.api.is_some(), "panic 后 API 必须归还");
    assert_eq!(manager.spawned_midi_max_port, 0, "失败不得提交布局");
    assert!(manager.needs_reinit(), "worker panic 应触发后端重初始化");
}

/// 同步路径 panic：API 保留、失败不提交布局，并触发后端重初始化兜底。
#[test]
fn sync_panic_keeps_api_and_requests_reinit() {
    let mut manager = manager_with(MockApi::panic_on_layout("注入的布局 panic").boxed());

    manager.apply_midi_port_layout(3);

    assert!(manager.api.is_some(), "同步 panic 后 API 必须保留");
    assert_eq!(manager.spawned_midi_max_port, 0, "panic 不得提交布局");
    assert!(manager.needs_reinit(), "同步 panic 应触发后端重初始化");
}

/// 非重初始化 drain 超时：保留在途，下一帧仍可回收 API。
#[test]
fn drain_timeout_keeps_inflight_for_recovery() {
    let mut manager = MidiManager::default();
    // 发送端保持存活但不发送：制造 recv_timeout 而非 Disconnected。
    let (_tx, rx) = mpsc::channel();
    manager.layout_apply = Some(LayoutApplyInFlight {
        rx,
        started: Instant::now(),
    });

    manager.drain_layout_apply_with_timeout(false, Duration::from_millis(5));

    assert!(manager.layout_apply.is_some(), "非重初始化超时应保留在途");
}

/// 重初始化 drain 超时：放弃在途，避免迟到的旧 API 覆盖新后端。
#[test]
fn reinit_drain_timeout_abandons_inflight() {
    let mut manager = MidiManager::default();
    let (_tx, rx) = mpsc::channel();
    manager.layout_apply = Some(LayoutApplyInFlight {
        rx,
        started: Instant::now(),
    });

    manager.drain_layout_apply_with_timeout(true, Duration::from_millis(5));

    assert!(manager.layout_apply.is_none(), "重初始化超时应放弃在途");
}

/// 通道在回传前断开（守卫未覆盖的进程级异常）：置 needs_reinit 兜底。
#[test]
fn disconnected_requests_reinit() {
    let mut manager = MidiManager::default();
    let (tx, rx) = mpsc::channel();
    drop(tx);
    manager.layout_apply = Some(LayoutApplyInFlight {
        rx,
        started: Instant::now(),
    });

    manager.poll_layout_apply();

    assert!(manager.needs_reinit(), "线程断开应触发后端重初始化兜底");
}
