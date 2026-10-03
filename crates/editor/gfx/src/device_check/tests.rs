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

/// 测试用回退适配器（Dx12）
fn dummy_fallback_adapter() -> GpuAdapterSummary {
    GpuAdapterSummary {
        name: "fallback".into(),
        backend: "Dx12".into(),
        device_type: "DiscreteGpu".into(),
        driver: "d".into(),
        driver_info: "i".into(),
    }
}

/// 验收标准 1 的确定性断言：**通过路径绝不触发回退后端枚举**。
///
/// `enumerate` 闭包注入计数，因此该断言不依赖机器是否有回退后端、也不依赖 GPU。
#[test]
fn test_fallback_adapters_on_failure_policy_skips_enumeration_when_passed() {
    let mut calls = 0usize;
    let collected = check::fallback_adapters_with(FallbackDiagnostics::OnFailure, true, || {
        calls += 1;
        vec![dummy_fallback_adapter()]
    });
    assert!(
        collected.is_empty(),
        "通过路径不得收集回退后端诊断（结果只进日志）"
    );
    assert_eq!(
        calls, 0,
        "通过路径不得调用适配器枚举闭包——这正是每次白花的数百毫秒"
    );
}

/// `Never`：通过/失败都不收集（启动静默路径）
#[test]
fn test_fallback_adapters_never_policy_skips_enumeration() {
    let mut calls = 0usize;
    let collected = check::fallback_adapters_with(FallbackDiagnostics::Never, false, || {
        calls += 1;
        vec![dummy_fallback_adapter()]
    });
    assert!(collected.is_empty(), "静默路径失败时也不收集");
    assert_eq!(calls, 0);
}

/// `OnFailure`：失败时收集（启动失败警告窗需要"可能仍可运行"提示）
#[test]
fn test_fallback_adapters_on_failure_policy_collects_when_failed() {
    let mut calls = 0usize;
    let collected = check::fallback_adapters_with(FallbackDiagnostics::OnFailure, false, || {
        calls += 1;
        vec![dummy_fallback_adapter()]
    });
    assert_eq!(calls, 1, "失败且要弹窗时必须收集一次");
    assert_eq!(collected.len(), 1);
}

/// `Always`：通过时也收集（设置页手动检测，人看得见结果与「复制诊断信息」）
#[test]
fn test_fallback_adapters_always_policy_collects_even_when_passed() {
    let mut calls = 0usize;
    let collected = check::fallback_adapters_with(FallbackDiagnostics::Always, true, || {
        calls += 1;
        vec![dummy_fallback_adapter()]
    });
    assert_eq!(calls, 1, "手动检测始终收集");
    assert_eq!(collected.len(), 1);
}

/// 端到端补强：真实全量检测走 `Never` 策略时回退列表为空。
///
/// **判别力说明**：本用例只断言「Never ⇒ 空」，而空是 `Never` 的构造保证，
/// 故它对详情段落逻辑没有判别力（旧版本还额外断言 `!detail().contains(…)`，
/// 那一条恒真）。段落逻辑的双向判别见
/// [`test_detail_fallback_section_follows_adapter_list`]。
#[test]
fn test_check_without_fallback_collection_returns_empty_list() {
    let report = run_check_with_timeout(DEFAULT_TIMEOUT, FallbackDiagnostics::Never);
    println!(
        "device_check（Never）本机结果: {}",
        report.detail().replace('\n', " | ")
    );
    assert_eq!(report.passed, report.failure.is_none());
    assert!(
        report.fallback_adapters.is_empty(),
        "Never 策略下不得返回回退后端适配器（实收 {} 个）",
        report.fallback_adapters.len()
    );
}

/// 详情中的「其他可用后端」段落必须**双向**跟随 `fallback_adapters`：
/// 非空时渲染、空时不渲染。
///
/// 这是旧断言（`Never ⇒ 无该段`）的正确替代：旧写法由构造恒真、零判别力，
/// 摘掉实现里的段落渲染逻辑它也不会变红。
#[test]
fn test_detail_fallback_section_follows_adapter_list() {
    let adapter = GpuAdapterSummary {
        name: "Microsoft Basic Render Driver".to_string(),
        backend: "Dx12".to_string(),
        device_type: "Cpu".to_string(),
        driver: String::new(),
        driver_info: String::new(),
    };

    let mut with_fallback = GpuCheckReport::failed(GpuCheckFailure::NoAdapter);
    with_fallback.fallback_adapters = vec![adapter];
    assert!(
        with_fallback.detail().contains("其他可用后端"),
        "回退列表非空时必须渲染提示段：{}",
        with_fallback.detail()
    );

    let without_fallback = GpuCheckReport::failed(GpuCheckFailure::NoAdapter);
    assert!(
        !without_fallback.detail().contains("其他可用后端"),
        "回退列表为空时不得渲染该段"
    );
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
    let report = run_check_with_timeout(DEFAULT_TIMEOUT, FallbackDiagnostics::Always);
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
