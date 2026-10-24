//! 配置持久化单元测试
//!
//! 覆盖两件事：语言落盘守卫（UI-020），以及「加载保护」两个相邻 bool
//! 在 `config_diff` / `apply_load_guard_to_config` 里的字段映射——写反不会
//! 报错，只会表现为"设置不落盘 / 开关不生效"。

use super::*;
use lumino_core::storage::config::UiConfig;
use lumino_ui::settings::SettingsPanel;

/// UI-020：语言落盘守卫真值表——env 来源且未改语言时不落盘，其余照常。
#[test]
fn test_should_persist_language_guard() {
    assert!(
        should_persist_language(false, false),
        "非 env 来源：照常落盘"
    );
    assert!(
        should_persist_language(false, true),
        "非 env 来源：照常落盘"
    );
    assert!(
        !should_persist_language(true, false),
        "env 来源且未改语言：不得物化（老配置场景的核心守卫）"
    );
    assert!(
        should_persist_language(true, true),
        "env 来源但用户显式改语言：照常落盘意图"
    );
}

/// 两个「加载保护」开关任一变化都必须被 `config_diff` 捕获（否则不落盘）。
#[test]
fn test_config_diff_detects_load_guard_toggles() {
    let old = UiConfig::default();
    let mut panel = SettingsPanel::new(&old);
    assert!(
        RunnerInner::config_diff(&panel, &old, &old.theme).is_none(),
        "面板镜像默认配置时不应产生差异"
    );

    panel.logging.load_memory_check_enabled = true;
    assert!(
        RunnerInner::config_diff(&panel, &old, &old.theme).is_some(),
        "加载期间内存检查开关变更必须触发落盘"
    );
    panel.logging.load_memory_check_enabled = false;

    panel.logging.load_watchdog_enabled = true;
    assert!(
        RunnerInner::config_diff(&panel, &old, &old.theme).is_some(),
        "加载期间进程强杀看门狗开关变更必须触发落盘"
    );
    panel.logging.load_watchdog_enabled = false;
    assert!(
        RunnerInner::config_diff(&panel, &old, &old.theme).is_none(),
        "恢复默认后不应再产生差异"
    );
}

/// 面板 → 配置的字段映射不得串位（两个相邻 bool 的经典静默 bug）。
#[test]
fn test_apply_load_guard_to_config_maps_fields() {
    let mut ui = UiConfig::default();
    let mut panel = SettingsPanel::new(&ui);
    panel.logging.load_memory_check_enabled = true;
    panel.logging.load_watchdog_enabled = false;
    apply_load_guard_to_config(&mut ui, &panel);
    assert!(
        ui.load_memory_check_enabled,
        "内存检查开关必须写入 load_memory_check_enabled"
    );
    assert!(!ui.load_watchdog_enabled, "看门狗开关不得串到内存检查字段");

    let mut ui = UiConfig::default();
    let mut panel = SettingsPanel::new(&ui);
    panel.logging.load_memory_check_enabled = false;
    panel.logging.load_watchdog_enabled = true;
    apply_load_guard_to_config(&mut ui, &panel);
    assert!(
        !ui.load_memory_check_enabled,
        "内存检查开关不得串到看门狗字段"
    );
    assert!(
        ui.load_watchdog_enabled,
        "看门狗开关必须写入 load_watchdog_enabled"
    );
}
