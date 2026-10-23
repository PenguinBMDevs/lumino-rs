//! GPU 检测报告 → UI 结论的投影（纯函数，便于单测）。
//!
//! `lumino-gfx` 的失败类型携带诊断字符串，UI 只需要类别。映射集中在此处：
//! 既保证 UI crate 不依赖图形栈，也保证「gfx 新增失败类型」必须在编译期显式
//! 落到某个 UI 结论上——本模块的 `match` 穷尽全部变体，不允许 `_` 兜底静默吞掉。

use lumino_gfx::device_check::{GpuCheckFailure, GpuCheckReport};
use lumino_ui::event::window::GpuCheckOutcome;

mod state;

pub(crate) use state::GpuCheckState;

/// 把检测报告折算为 UI 结论。
///
/// 判定优先级：**失败原因优先于 `passed` 布尔值**。二者理论一致
/// （`passed == failure.is_none()`），但 `GpuCheckReport` 是公开结构体，手工构造
/// 可能出现矛盾；此时按失败原因展示，并把「未通过且无原因」归为
/// [`GpuCheckOutcome::Unknown`]——绝不让 UI 误显示为「通过」。
pub(crate) fn outcome_of(report: &GpuCheckReport) -> GpuCheckOutcome {
    match &report.failure {
        Some(GpuCheckFailure::NoAdapter) => GpuCheckOutcome::NoAdapter,
        Some(GpuCheckFailure::DeviceRequest(_)) => GpuCheckOutcome::DeviceRequestFailed,
        Some(GpuCheckFailure::RenderError(_)) => GpuCheckOutcome::RenderFailed,
        Some(GpuCheckFailure::Timeout) => GpuCheckOutcome::TimedOut,
        Some(GpuCheckFailure::Aborted(_)) => GpuCheckOutcome::ProbeFailed,
        None if report.passed => GpuCheckOutcome::Passed,
        None => GpuCheckOutcome::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 五类失败必须映射到五个**互不相同**的 UI 结论——
    /// 这是「超时 / 驱动错误 / 检测环境异常在 UI 上可区分」的契约底线。
    #[test]
    fn test_every_failure_kind_maps_to_distinct_outcome() {
        let cases = [
            (GpuCheckFailure::NoAdapter, GpuCheckOutcome::NoAdapter),
            (
                GpuCheckFailure::DeviceRequest("boom".into()),
                GpuCheckOutcome::DeviceRequestFailed,
            ),
            (
                GpuCheckFailure::RenderError("blank".into()),
                GpuCheckOutcome::RenderFailed,
            ),
            (GpuCheckFailure::Timeout, GpuCheckOutcome::TimedOut),
            (
                GpuCheckFailure::Aborted("child crashed".into()),
                GpuCheckOutcome::ProbeFailed,
            ),
        ];

        let mut seen = std::collections::BTreeSet::new();
        for (failure, expected) in cases {
            let report = GpuCheckReport::failed(failure);
            let outcome = outcome_of(&report);
            assert_eq!(outcome, expected, "失败类型与 UI 结论映射错误");
            assert!(!outcome.is_passed(), "失败报告不得被判定为通过");
            seen.insert(format!("{outcome:?}"));
        }
        assert_eq!(seen.len(), 5, "五类失败必须在 UI 侧保持可区分");
    }

    /// 通过路径：无失败原因 + passed=true
    #[test]
    fn test_passed_report_maps_to_passed() {
        let report = GpuCheckReport {
            passed: true,
            failure: None,
            required_backend: "Vulkan",
            adapters: Vec::new(),
            fallback_adapters: Vec::new(),
            fingerprint: Some("Vulkan|gpu|drv|info|DiscreteGpu".into()),
            duration_ms: 42,
        };
        let outcome = outcome_of(&report);
        assert_eq!(outcome, GpuCheckOutcome::Passed);
        assert!(outcome.is_passed());
    }

    /// 自相矛盾的报告（passed=false 且无失败原因）不得显示为通过
    #[test]
    fn test_inconsistent_report_is_unknown_not_passed() {
        let report = GpuCheckReport {
            passed: false,
            failure: None,
            required_backend: "Vulkan",
            adapters: Vec::new(),
            fallback_adapters: Vec::new(),
            fingerprint: None,
            duration_ms: 0,
        };
        let outcome = outcome_of(&report);
        assert_eq!(outcome, GpuCheckOutcome::Unknown);
        assert!(!outcome.is_passed(), "无原因失败不能被当作通过");
    }

    /// 失败原因优先：即使 `passed` 被误置为 true，也不得展示为通过
    #[test]
    fn test_failure_takes_precedence_over_passed_flag() {
        let report = GpuCheckReport {
            passed: true,
            failure: Some(GpuCheckFailure::Timeout),
            required_backend: "Vulkan",
            adapters: Vec::new(),
            fallback_adapters: Vec::new(),
            fingerprint: None,
            duration_ms: 3000,
        };
        assert_eq!(outcome_of(&report), GpuCheckOutcome::TimedOut);
    }

    /// 只有 `Passed` 一个变体算通过（防止未来新增变体时漏改判定）
    #[test]
    fn test_only_passed_variant_counts_as_passed() {
        for outcome in [
            GpuCheckOutcome::NoAdapter,
            GpuCheckOutcome::DeviceRequestFailed,
            GpuCheckOutcome::RenderFailed,
            GpuCheckOutcome::TimedOut,
            GpuCheckOutcome::ProbeFailed,
            GpuCheckOutcome::Unknown,
        ] {
            assert!(!outcome.is_passed(), "{outcome:?} 不应被判定为通过");
        }
        assert!(GpuCheckOutcome::Passed.is_passed());
    }
}
