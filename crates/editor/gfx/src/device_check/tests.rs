//! `device_check` 单元测试。
//!
//! 无 GPU 的 CI 环境走失败路径断言（不 panic）；有 GPU 的环境走通过路径断言。
//! 一律使用带超时的入口，避免异常驱动导致测试挂死。

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
