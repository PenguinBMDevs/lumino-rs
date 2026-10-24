//! REND-019 R1：wgpu 级注入（`device.destroy()`）——lost 回调 → 原子标志链路。
//!
//! ⚠️ 必须使用**独立 device**：禁止 `test_gpu.rs` 的进程级共享设备——
//! `destroy()` 会毒化同进程其他测试（共享设备不可恢复地丢失）。
//!
//! 进程级静态标志与并行测试互斥：本文件的测试统一持 [`FLAG_TEST_LOCK`]
//! 串行执行，防「读—清零」与其他用例的「置位」互相踩踏。

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::{device_lost, ensure_device_available, mark_device_lost, reset_for_test};

/// 全局标志相关测试的串行锁（测试进程内并行线程互斥）。
static FLAG_TEST_LOCK: Mutex<()> = Mutex::new(());

/// 无论是否中毒都能取锁（测试互斥，不关心中毒）
fn lock_flag_tests() -> MutexGuard<'static, ()> {
    FLAG_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 纯逻辑：标志置位 / 读取 / 幂等。
#[test]
fn test_device_lost_flag_set_and_read() {
    let _guard = lock_flag_tests();
    reset_for_test();
    assert!(!device_lost(), "复位后应为 false");
    assert!(mark_device_lost(), "首次置位应返回 true（首置语义）");
    assert!(device_lost(), "置位后应为 true");
    assert!(
        !mark_device_lost(),
        "二次置位应返回 false（幂等，供调用方收敛告警）"
    );
    assert!(device_lost());
    reset_for_test();
    assert!(!device_lost(), "测试必须复位，避免串扰");
}

/// R1：独立设备 `destroy()` → 回调触发、标志置位、reason 为 `Destroyed`。
#[test]
fn test_reconnect_r1_destroy_triggers_callback_and_flag() {
    use futures::executor::block_on;

    let _guard = lock_flag_tests();
    reset_for_test();

    // ── 独立 instance / adapter / device（不带 surface；CI 软渲染同样可用）──
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("独立适配器应可用（硬件或 CI 软渲染）");
    let (device, _queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("reconnect_r1_injection_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("独立设备请求失败");

    // 销毁路径可能伴随各类错误回调——安装非 panic 处理器，保证测试只断言目标链路
    device.on_uncaptured_error(Arc::new(|err| {
        eprintln!("[R1 注入测试] 捕获非致命错误: {err}");
    }));

    // ── 挂回调：捕获 reason + 走生产接线路径（只做原子置位，红线 C1）──
    let captured: Arc<Mutex<Vec<(wgpu::DeviceLostReason, String)>>> =
        Arc::new(Mutex::new(Vec::new()));
    {
        let captured = Arc::clone(&captured);
        device.set_device_lost_callback(move |reason, message| {
            if let Ok(mut v) = captured.lock() {
                v.push((reason, message.clone()));
            }
            super::on_device_lost(reason, message);
        });
    }

    // ── 注入：显式销毁设备（与生产 device lost 回调同一条接线）──
    device.destroy();

    // 跨后端轮询等待回调（硬件驱动与软渲染事件时机不同）
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        let done = captured.lock().map(|v| !v.is_empty()).unwrap_or(false);
        if done || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let events = captured.lock().expect("回调捕获锁不应中毒").clone();
    assert!(
        !events.is_empty(),
        "destroy() 后应在 5s 内触发 device-lost 回调"
    );
    assert!(
        events
            .iter()
            .any(|(reason, _)| *reason == wgpu::DeviceLostReason::Destroyed),
        "reason 应为 Destroyed，实际: {events:?}"
    );
    assert!(device_lost(), "回调未置位全局 DEVICE_LOST 标志");

    reset_for_test();
}

/// R1 扩展（REND-019 PR-C present 硬化前置不变量）：
/// `destroy()` 后**单次 `poll` 必须同步送达 device-lost 回调**（置位停帧标志）。
///
/// 实测背景：`poll` 对丢失设备返回 `Ok(QueueEmpty)`（不报错）——present 前的
/// 探测不能依赖其返回值，只能依赖「poll 处理阶段同步触发回调」这一副作用。
/// 若此不变量不成立，present 硬化需更换探测通道。
#[test]
fn test_reconnect_r1_poll_delivers_lost_callback_sync() {
    use futures::executor::block_on;

    let _guard = lock_flag_tests();
    reset_for_test();

    // 独立设备（禁共享，同 R1 约束）
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("独立适配器应可用（硬件或 CI 软渲染）");
    let (device, _queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("r1_poll_probe_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("独立设备请求失败");

    device.on_uncaptured_error(Arc::new(|err| {
        eprintln!("[R1 poll 探测测试] 捕获错误: {err}");
    }));

    let captured: Arc<Mutex<Vec<wgpu::DeviceLostReason>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let captured = Arc::clone(&captured);
        device.set_device_lost_callback(move |reason, _message| {
            if let Ok(mut v) = captured.lock() {
                v.push(reason);
            }
            let _ = super::mark_device_lost();
        });
    }

    device.destroy();

    // 关键断言：单次 poll（返回 Ok 也无妨）即应同步送达回调 → 停帧标志置位
    let poll_result = device.poll(wgpu::PollType::Poll);
    let delivered = captured.lock().map(|v| !v.is_empty()).unwrap_or(false);
    assert!(
        delivered,
        "destroy 后单次 poll 应同步送达 lost 回调（poll 结果: {poll_result:?}）"
    );
    assert!(
        device_lost(),
        "回调应在 poll 轮询内同步置位全局 DEVICE_LOST 标志"
    );

    reset_for_test();
}

/// REND-020：统一守卫的纯逻辑——置位拒绝、复位放行。
#[test]
fn test_rend_020_guard_rejects_when_device_lost() {
    let _guard = lock_flag_tests();
    reset_for_test();
    assert!(ensure_device_available().is_ok(), "初始应允许");
    assert!(mark_device_lost(), "首次置位应返回 true");
    assert!(
        matches!(
            ensure_device_available(),
            Err(crate::ContextError::DeviceLost)
        ),
        "停帧标志置位后守卫必须返回 DeviceLost"
    );
    reset_for_test();
    assert!(ensure_device_available().is_ok(), "复位后应恢复");
}

/// REND-020：`Context::new` 必须在创建 surface 之前早退（用 panic factory 证明）。
#[test]
fn test_rend_020_context_new_rejects_before_surface_creation() {
    use futures::executor::block_on;

    let _guard = lock_flag_tests();
    reset_for_test();
    assert!(mark_device_lost(), "首次置位应返回 true");

    let factory = |_: &wgpu::Instance| -> std::result::Result<
        wgpu::Surface<'static>,
        wgpu::CreateSurfaceError,
    > {
        panic!("REND-020：停帧守卫必须先于 surface 创建");
    };
    let result = block_on(crate::Context::new(factory, 320, 240));
    assert!(
        matches!(result, Err(crate::ContextError::DeviceLost)),
        "停帧窗口内 Context::new 必须返回 DeviceLost"
    );

    reset_for_test();
}
