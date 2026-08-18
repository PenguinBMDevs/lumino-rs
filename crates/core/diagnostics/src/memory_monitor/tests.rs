//! MemoryMonitor 单元测试

use super::*;

#[test]
fn test_memory_monitor_defaults() {
    let monitor = MemoryMonitor::new();
    assert!(monitor.total_physical() > 0);
    assert!(monitor.soft_limit() > 0);
    assert_eq!(
        monitor.soft_limit(),
        monitor
            .total_physical()
            .saturating_sub(DEFAULT_RESERVE_BYTES)
    );
    assert_eq!(
        monitor
            .rss_fail_count
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
}

/// macOS 上跳过：部分受限环境（CI/沙箱）下 `task_info` 调用失败返回 0，
/// 导致 RSS 断言必然失败。macOS 的 RSS 读取已由
/// [`platform::test_macos_task_basic_info_layout`] 覆盖结构体布局正确性，
/// 运行时取值在 macOS 上按「失败返回 0 → 容错跳过」处理。
#[cfg(not(target_os = "macos"))]
#[test]
fn test_current_rss_returns_nonzero() {
    let rss = platform::get_current_rss();
    assert!(
        rss > 0,
        "RSS 应该 > 0（Linux: /proc/self/status, macOS: task_info, Windows: GetProcessMemoryInfo）"
    );
}

/// macOS 上跳过：依赖 [`MemoryMonitor::current_rss()`]，RSS 为 0 时
/// `usage_ratio()` 返回 0.0，必然不满足 (0, 1) 区间断言，原因同上。
#[cfg(not(target_os = "macos"))]
#[test]
fn test_usage_ratio() {
    let monitor = MemoryMonitor::new();
    let ratio = monitor.usage_ratio();
    assert!(
        ratio > 0.0 && ratio < 1.0,
        "内存使用率应该在 0~1 之间，实际值为 {}",
        ratio
    );
}

#[test]
fn test_global_is_singleton() {
    let instance_a = MemoryMonitor::global() as *const MemoryMonitor;
    let instance_b = MemoryMonitor::global() as *const MemoryMonitor;
    assert_eq!(instance_a, instance_b, "global() 应该返回相同实例");
}

#[test]
fn test_check_inner_normal() {
    let monitor = MemoryMonitor::new();
    let check_result = monitor.check_inner("test: ");
    assert_eq!(
        check_result,
        Some(false),
        "正常状态不应超限（若 RSS 读取失败也可能是 None，但不会是 Some(true)）"
    );
}
