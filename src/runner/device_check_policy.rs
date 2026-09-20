//! 启动 GPU 设备检查策略（纯函数，便于单测）。
//!
//! "是否执行检查"与"失败是否弹窗"是两个独立维度：
//! - `run_check`：设置页"每次启动检查 GPU 兼容性"开关（默认开启）
//! - `show_warning`：`gpu_warning_suppressed` 不为 `Some(true)` 时弹窗；
//!   全新配置（`None`）按未抑制处理，旧版配置在 Storage 加载时已归一化为 `Some(true)`
//!
//! 另含检测缓存的时间有效期（TTL）判定：适配器指纹在 macOS（Metal）上恒为常量
//! （`driver` / `driver_info` 均为空串），仅靠指纹无法感知系统 / 驱动大版本升级，
//! 因此缓存必须带时间兜底（见 `cache_within_ttl` / `cache_hit`）。

use lumino_core::storage::config::UiConfig;

/// 启动检查策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StartupCheckPolicy {
    /// 是否执行检测
    pub(crate) run_check: bool,
    /// 检测失败时是否弹警告窗
    pub(crate) show_warning: bool,
}

/// 检测缓存有效期：7 天。
///
/// 平台无关的兜底：Windows / Linux 的指纹本身带驱动版本，TTL 只是额外的安全网；
/// macOS 上则是缓存唯一的失效途径。
pub(crate) const GPU_CHECK_CACHE_TTL_DAYS: u64 = 7;

/// 检测缓存有效期（Unix 秒）
pub(crate) const GPU_CHECK_CACHE_TTL_SECS: u64 = GPU_CHECK_CACHE_TTL_DAYS * 24 * 60 * 60;

/// 从配置解析启动检查策略
pub(crate) fn resolve_policy(config: &UiConfig) -> StartupCheckPolicy {
    StartupCheckPolicy {
        run_check: config.gpu_check_on_startup,
        show_warning: !matches!(config.gpu_warning_suppressed, Some(true)),
    }
}

/// 判定"上次检测时间戳"是否仍在 TTL 内。
///
/// - `last_check_time`：`Some` = 上次检测的 Unix 秒；`None` = 旧配置缺该字段 → 视为过期
/// - `now`：当前 Unix 秒；`None` = 系统时钟异常（早于 epoch）→ 视为过期
///
/// 过期区间为半开区间 `[last, last + TTL)`：恰好满 TTL 即算过期。
/// 时间戳晚于当前时间（用户回拨系统时钟）按未过期处理，避免误判后每次启动都全量检测。
pub(crate) fn cache_within_ttl(last_check_time: Option<u64>, now: Option<u64>) -> bool {
    let (Some(last), Some(now)) = (last_check_time, now) else {
        return false;
    };
    now.saturating_sub(last) < GPU_CHECK_CACHE_TTL_SECS
}

/// 调试开关：强制把检测缓存视为过期，用于验证"超过 TTL 强制全量检测"（仅 debug 构建）。
///
/// 无需等待 7 天或手改 config.json；效果等价于把 `gpu_last_check_time` 改成一个过期时间戳。
pub(crate) fn debug_expire_cache_requested() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var_os("LUMINO_DEBUG_STALE_GPU_CACHE").is_some()
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

/// 综合判定指纹缓存是否命中（纯函数；廉价探测以闭包注入，便于单测）。
///
/// 命中需同时满足：
/// 1. `within_ttl`：缓存时间戳在 TTL 内（旧配置 / 已过期一律不命中）
/// 2. 未开启"强制检测失败"调试开关
/// 3. 上次检测通过
/// 4. 廉价探测仍能枚举出与缓存一致的适配器指纹
///
/// 条件 1~3 任一不成立时 **不会调用 `probe`**：TTL 过期直接落回全量检测，
/// 连新建 wgpu `Instance` 的廉价探测都省掉。
pub(crate) fn cache_hit<P>(
    within_ttl: bool,
    force_fail: bool,
    cached_passed: Option<bool>,
    cached_fingerprint: Option<&str>,
    probe: P,
) -> bool
where
    P: FnOnce() -> Vec<String>,
{
    if !within_ttl || force_fail || cached_passed != Some(true) {
        return false;
    }
    let Some(fingerprint) = cached_fingerprint else {
        return false;
    };
    probe().iter().any(|probed| probed == fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// 测试用"当前时间"，固定值避免时钟抖动带来的边界不稳定
    const NOW: u64 = 1_700_000_000;
    /// macOS Metal 上的恒定指纹（`driver` / `driver_info` 为空串）——正是本 TTL 要兜底的场景
    const MAC_FINGERPRINT: &str = "Metal|Apple M2|||IntegratedGpu";

    #[test]
    fn test_fresh_config_runs_and_warns() {
        let policy = resolve_policy(&UiConfig::default());
        assert!(policy.run_check, "全新配置默认应执行检查");
        assert!(policy.show_warning, "全新配置默认应允许弹窗");
    }

    #[test]
    fn test_suppressed_config_still_runs_but_silent() {
        let config = UiConfig {
            gpu_warning_suppressed: Some(true),
            ..UiConfig::default()
        };
        let policy = resolve_policy(&config);
        assert!(policy.run_check, "抑制弹窗不影响检查执行");
        assert!(!policy.show_warning);
    }

    #[test]
    fn test_check_disabled_skips_check() {
        let config = UiConfig {
            gpu_check_on_startup: false,
            ..UiConfig::default()
        };
        let policy = resolve_policy(&config);
        assert!(!policy.run_check);
    }

    #[test]
    fn test_legacy_normalized_value_is_silent() {
        // 旧版配置在 Storage 加载时被归一化为 Some(true)
        let config = UiConfig {
            gpu_warning_suppressed: Some(true),
            ..UiConfig::default()
        };
        assert!(!resolve_policy(&config).show_warning);
    }

    #[test]
    fn test_cache_within_ttl_fresh_timestamp_is_valid() {
        assert!(
            cache_within_ttl(Some(NOW - 60), Some(NOW)),
            "1 分钟前的检测应仍在 TTL 内"
        );
        assert!(
            cache_within_ttl(Some(NOW - GPU_CHECK_CACHE_TTL_SECS + 1), Some(NOW)),
            "TTL 边界内 1 秒应仍有效"
        );
    }

    #[test]
    fn test_cache_within_ttl_boundary_and_beyond_is_expired() {
        assert!(
            !cache_within_ttl(Some(NOW - GPU_CHECK_CACHE_TTL_SECS), Some(NOW)),
            "恰好满 TTL 即视为过期"
        );
        assert!(
            !cache_within_ttl(Some(NOW - GPU_CHECK_CACHE_TTL_SECS - 1), Some(NOW)),
            "超过 TTL 1 秒应视为过期"
        );
        assert!(
            !cache_within_ttl(Some(0), Some(NOW)),
            "1970 年的时间戳等价于无缓存"
        );
    }

    #[test]
    fn test_cache_within_ttl_missing_timestamp_is_expired() {
        // 旧配置缺 gpu_last_check_time 字段 → miss，不 panic、不误命中
        assert!(!cache_within_ttl(None, Some(NOW)));
    }

    #[test]
    fn test_cache_within_ttl_broken_clock_is_expired() {
        // 系统时钟早于 epoch：时间不可信，按过期处理（宁可多检不漏检）
        assert!(!cache_within_ttl(Some(NOW), None));
        assert!(!cache_within_ttl(None, None));
    }

    #[test]
    fn test_cache_within_ttl_future_timestamp_not_expired() {
        // 用户回拨系统时钟：不得因"未来时间戳"反复触发全量检测
        assert!(cache_within_ttl(Some(NOW + 86_400), Some(NOW)));
    }

    #[test]
    fn test_cache_hit_when_ttl_fresh_and_fingerprint_matches() {
        let probe_calls = Cell::new(0);
        let hit = cache_hit(true, false, Some(true), Some(MAC_FINGERPRINT), || {
            probe_calls.set(probe_calls.get() + 1);
            vec![MAC_FINGERPRINT.to_string()]
        });
        assert!(hit, "TTL 内且指纹一致时应命中缓存（走廉价探测）");
        assert_eq!(probe_calls.get(), 1, "TTL 内必须执行一次廉价探测比对指纹");
    }

    #[test]
    fn test_cache_hit_expired_ttl_forces_full_check() {
        // 核心回归：macOS 上指纹恒定，TTL 过期必须 miss —— 否则检测形同虚设
        let probe_calls = Cell::new(0);
        let hit = cache_hit(false, false, Some(true), Some(MAC_FINGERPRINT), || {
            probe_calls.set(probe_calls.get() + 1);
            vec![MAC_FINGERPRINT.to_string()]
        });
        assert!(!hit, "TTL 过期时指纹再一致也不得命中，必须全量检测");
        assert_eq!(
            probe_calls.get(),
            0,
            "TTL 过期时应跳过廉价探测，直接落回全量检测"
        );
    }

    #[test]
    fn test_cache_hit_miss_when_fingerprint_changed() {
        let hit = cache_hit(true, false, Some(true), Some(MAC_FINGERPRINT), || {
            vec!["Metal|Apple M4|||IntegratedGpu".to_string()]
        });
        assert!(!hit, "指纹变化（换卡/加驱动版本）应落回全量检测");
    }

    #[test]
    fn test_cache_hit_requires_passed_and_no_force_fail() {
        assert!(
            !cache_hit(true, false, Some(false), Some(MAC_FINGERPRINT), Vec::new),
            "上次未通过不得命中缓存"
        );
        assert!(
            !cache_hit(true, false, None, Some(MAC_FINGERPRINT), Vec::new),
            "无检测结果不得命中缓存"
        );
        let probe_calls = Cell::new(0);
        let hit = cache_hit(true, true, Some(true), Some(MAC_FINGERPRINT), || {
            probe_calls.set(probe_calls.get() + 1);
            vec![MAC_FINGERPRINT.to_string()]
        });
        assert!(!hit, "调试强制失败开关必须禁用缓存");
        assert_eq!(probe_calls.get(), 0, "强制失败时不应执行廉价探测");
    }

    #[test]
    fn test_cache_hit_missing_cached_fingerprint_is_miss() {
        assert!(!cache_hit(true, false, Some(true), None, Vec::new));
    }
}
