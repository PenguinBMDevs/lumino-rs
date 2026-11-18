//! REND-019 R1：wgpu 级注入（`device.destroy()`）——lost 回调 → 原子标志链路。
//!
//! ⚠️ 必须使用**独立 device**：禁止 `test_gpu.rs` 的进程级共享设备——
//! `destroy()` 会毒化同进程其他测试（共享设备不可恢复地丢失）。
//!
//! 进程级静态标志与并行测试互斥：本文件的测试统一持 [`FLAG_TEST_LOCK`]
//! 串行执行，防「读—清零」与其他用例的「置位」互相踩踏。

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::{device_lost, ensure_device_available, mark_device_lost, reset_for_test, safe_unmap};

/// 全局标志相关测试的串行锁（测试进程内并行线程互斥）。
static FLAG_TEST_LOCK: Mutex<()> = Mutex::new(());

/// 无论是否中毒都能取锁（测试互斥，不关心中毒）
fn lock_flag_tests() -> MutexGuard<'static, ()> {
    FLAG_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 申请**独立**实例 + 适配器 + 设备（禁共享设备，同 R1 约束）。
///
/// Windows DX12/WARP 在「并发建/销 Device」的风暴期（本 crate 的共享设备、
/// `device_check` 探测、`destroy()` 注入用例同进程并行）可能让 `request_adapter`
/// 瞬时返回 `NotFound`、或让 `request_device` 瞬时返回 `Device(Lost)`——
/// 2026-11 CI 两轮实测分别命中这两处（`test_gpu.rs` 记载过同类 DX12 停摆表现）。
/// 此处对 adapter 与 device 均重试 3 次吸收**瞬时**失败，并支持
/// `LUMINO_GFX_TEST_FALLBACK` 软渲染兜底（与 test_gpu / transient_device 一致）。
/// 重试仍败说明 DX12 栈已常驻停摆，属环境级故障，panic 消息带出末次错误便于定位。
fn request_independent_device(label: &'static str) -> (wgpu::Device, wgpu::Queue) {
    use futures::executor::block_on;

    // 与 test_gpu / cpu_onion_renderer::tests_support 一致：支持显式软渲染兜底，
    // 并对 adapter 申请重试（DX12 栈在并发建/销 Device 风暴期会瞬时返回 NotFound，
    // 2026-11 CI 实测：失败点恰在 request_adapter 而 device 侧已有重试）。
    let force_fallback = std::env::var_os("LUMINO_GFX_TEST_FALLBACK").is_some();
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = {
        let mut found = None;
        let mut last_err = None;
        for attempt in 1..=3 {
            match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter: force_fallback,
                ..Default::default()
            })) {
                Ok(adapter) => {
                    found = Some(adapter);
                    break;
                }
                Err(err) => {
                    eprintln!("[device_status 测试] request_adapter 第 {attempt}/3 次失败: {err}");
                    last_err = Some(err);
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
        found.unwrap_or_else(|| {
            panic!(
                "独立适配器应可用（硬件、CI 软渲染或重试后）: {:?}",
                last_err
            )
        })
    };

    let descriptor = || wgpu::DeviceDescriptor {
        label: Some(label),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    };
    let mut last_err: Option<wgpu::RequestDeviceError> = None;
    for attempt in 1..=3 {
        match block_on(adapter.request_device(&descriptor())) {
            Ok(pair) => return pair,
            Err(err) => {
                eprintln!("[device_status 测试] request_device 第 {attempt}/3 次失败: {err}");
                last_err = Some(err);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    panic!(
        "独立设备请求失败（3 次重试，DX12 栈可能已常驻停摆）: {}",
        last_err.expect("至少失败过一次")
    );
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
    let _guard = lock_flag_tests();
    reset_for_test();

    // ── 独立设备（不带 surface；CI 软渲染同样可用；重试抗 DX12 瞬时风暴）──
    let (device, _queue) = request_independent_device("reconnect_r1_injection_device");

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
    let _guard = lock_flag_tests();
    reset_for_test();

    // 独立设备（禁共享，同 R1 约束；重试抗 DX12 瞬时风暴）
    let (device, _queue) = request_independent_device("r1_poll_probe_device");

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

/// REND-022：`readback_has_content` 的 map→unmap 窗口真实存在——存在未释放的
/// mapped view 时，裸 `unmap` 会命中 wgpu facade 的断言 panic；`safe_unmap`
/// 必须吞掉它、安静返回。
#[test]
fn test_rend_022_safe_unmap_swallows_mapped_view_panic() {
    let _guard = lock_flag_tests();
    reset_for_test();

    // 独立设备（禁共享，同 R1 约束；重试抗 DX12 瞬时风暴）
    let (device, _queue) = request_independent_device("r5b_safe_unmap_device");
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("r5b_safe_unmap_readback"),
        size: 256,
        usage: wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // 完成映射（内容不参与断言，只验证 unmap 语义）
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(5)),
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("map_async 回调应在超时前送达")
        .expect("缓冲区映射应成功");

    // 持有 mapped view 不释放：这正是 R5-b 的 panic 条件
    //（`MapContext::reset` 断言 "cannot unmap a buffer that still has accessible mapped views"）
    let view = buffer.slice(..).get_mapped_range();

    let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| buffer.unmap()));
    assert!(
        raw.is_err(),
        "存在未释放的 mapped view 时裸 unmap 应 panic（R5-b 竞态窗口的前提不成立，需重查 wgpu 版本行为）"
    );

    // 目标断言：safe_unmap 必须吞掉 panic、安静返回
    safe_unmap(&buffer);

    drop(view);
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
