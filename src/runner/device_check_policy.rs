//! 启动 GPU 设备检查策略（纯函数，便于单测）。
//!
//! "是否执行检查"与"失败是否弹窗"是两个独立维度：
//! - `run_check`：设置页"每次启动检查 GPU 兼容性"开关（默认开启）
//! - `show_warning`：`gpu_warning_suppressed` 不为 `Some(true)` 时弹窗；
//!   全新配置（`None`）按未抑制处理，旧版配置在 Storage 加载时已归一化为 `Some(true)`

use lumino_core::storage::config::UiConfig;

/// 启动检查策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StartupCheckPolicy {
    /// 是否执行检测
    pub(crate) run_check: bool,
    /// 检测失败时是否弹警告窗
    pub(crate) show_warning: bool,
}

/// 从配置解析启动检查策略
pub(crate) fn resolve_policy(config: &UiConfig) -> StartupCheckPolicy {
    StartupCheckPolicy {
        run_check: config.gpu_check_on_startup,
        show_warning: !matches!(config.gpu_warning_suppressed, Some(true)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
