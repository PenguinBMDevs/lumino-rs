//! `device_check` 单元测试。
//!
//! 无 GPU 的 CI 环境走失败路径断言（不 panic）；有 GPU 的环境走通过路径断言。
//! 一律使用带超时的入口，避免异常驱动导致测试挂死。

use std::time::Duration;

use super::*;

#[test]
fn test_required_backend_name_matches_platform() {
    #[cfg(target_os = "macos")]
    assert_eq!(required_backend_name(), "Metal");
    #[cfg(not(target_os = "macos"))]
    assert_eq!(required_backend_name(), "Vulkan");
}

#[test]
fn test_fingerprint_is_stable_and_sensitive() {
    let base = GpuAdapterSummary {
        name: "Test GPU".into(),
        backend: "Vulkan".into(),
        device_type: "DiscreteGpu".into(),
        driver: "driver".into(),
        driver_info: "info".into(),
    };
    assert_eq!(base.fingerprint(), base.clone().fingerprint());
    let mut changed = base.clone();
    changed.driver = "other-driver".into();
    assert_ne!(base.fingerprint(), changed.fingerprint());
}

#[test]
fn test_failed_report_sets_invariants() {
    let report = GpuCheckReport::failed(GpuCheckFailure::Timeout);
    assert!(!report.passed);
    assert_eq!(report.failure, Some(GpuCheckFailure::Timeout));
    assert_eq!(report.required_backend, required_backend_name());
    assert!(report.detail().contains("检测超时"));
}

#[test]
fn test_probe_with_timeout_returns_value_on_completion() {
    let result = check::probe_with_timeout(Duration::from_millis(500), || vec!["fp".to_string()]);
    assert_eq!(result, Some(vec!["fp".to_string()]));
}

#[test]
fn test_probe_with_timeout_returns_none_on_hang() {
    // 模拟驱动挂起：探测线程睡 500ms，超时 50ms → 必须立即返回 None
    // （调用方按 cache miss 落回全量检测，UI-008 / #37）
    let started = std::time::Instant::now();
    let result = check::probe_with_timeout(Duration::from_millis(50), || {
        std::thread::sleep(Duration::from_millis(500));
        vec!["never-returned".to_string()]
    });
    assert!(
        result.is_none(),
        "探测挂起超时必须返回 None（调用方按 cache miss 处理）"
    );
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "超时应立即返回，不得等待探测线程结束（实测 {:?}）",
        started.elapsed()
    );
}

#[test]
fn test_probe_with_timeout_thread_panic_is_none() {
    let result = check::probe_with_timeout(Duration::from_millis(500), || {
        panic!("模拟探测线程 panic");
    });
    assert!(
        result.is_none(),
        "探测线程 panic（channel 断开）必须按 None 处理"
    );
}

#[test]
fn test_timeout_wrapper_returns_wellformed_report() {
    let report = run_check_with_timeout(DEFAULT_TIMEOUT);
    // 本机结果（--nocapture 可见）：便于人工确认无头试画路径真实可跑
    println!(
        "device_check 本机结果: {}",
        report.detail().replace('\n', " | ")
    );
    // 结构不变量：passed 与 failure 必须互补，且报告文本非空
    assert_eq!(report.passed, report.failure.is_none());
    assert!(!report.detail().is_empty());
    if report.passed {
        assert!(report.fingerprint.is_some());
    }
}
