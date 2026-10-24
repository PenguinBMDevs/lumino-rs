//! 设置面板单元测试

use super::*;
use lumino_core::storage::config::UiConfig;

fn panel_with_tempo(max_bpm: f64) -> SettingsPanel {
    let config = UiConfig {
        tempo_max_bpm: max_bpm,
        ..Default::default()
    };
    SettingsPanel::new(&config)
}

#[test]
fn test_tempo_max_bpm_default_from_config() {
    let panel = panel_with_tempo(512.0);
    assert_eq!(panel.editing.tempo_max_bpm, 512.0);
    assert!(!panel.editing.tempo_custom_open);
}

#[test]
fn test_tempo_preset_selected_closes_custom_panel() {
    let mut panel = panel_with_tempo(512.0);
    panel.update(Event::TempoMaxBpmCustomOpen);
    assert!(panel.editing.tempo_custom_open);
    panel.update(Event::TempoMaxBpmChanged(2048.0));
    assert_eq!(panel.editing.tempo_max_bpm, 2048.0);
    assert!(!panel.editing.tempo_custom_open);
}

#[test]
fn test_tempo_custom_open_prefills_current_value() {
    let mut panel = panel_with_tempo(700.0);
    panel.update(Event::TempoMaxBpmCustomOpen);
    assert!(panel.editing.tempo_custom_open);
    assert_eq!(panel.editing.tempo_custom_input, "700");
}

#[test]
fn test_tempo_custom_input_and_confirm() {
    let mut panel = panel_with_tempo(512.0);
    panel.update(Event::TempoMaxBpmCustomOpen);
    panel.update(Event::TempoMaxBpmCustomInput("1234".to_string()));
    panel.update(Event::TempoMaxBpmCustomConfirm);
    assert_eq!(panel.editing.tempo_max_bpm, 1234.0);
    assert!(!panel.editing.tempo_custom_open);
}

#[test]
fn test_tempo_custom_confirm_invalid_keeps_value() {
    let mut panel = panel_with_tempo(512.0);
    panel.update(Event::TempoMaxBpmCustomOpen);
    panel.update(Event::TempoMaxBpmCustomInput("abc".to_string()));
    panel.update(Event::TempoMaxBpmCustomConfirm);
    // 无效输入不生效，面板保持打开以便修正
    assert_eq!(panel.editing.tempo_max_bpm, 512.0);
    assert!(panel.editing.tempo_custom_open);
}

#[test]
fn test_tempo_custom_close() {
    let mut panel = panel_with_tempo(512.0);
    panel.update(Event::TempoMaxBpmCustomOpen);
    panel.update(Event::TempoMaxBpmCustomClose);
    assert!(!panel.editing.tempo_custom_open);
}

// ── 兼容性设置 ──

#[test]
fn test_compat_init_from_config() {
    let config = UiConfig {
        gpu_check_on_startup: false,
        gpu_warning_suppressed: Some(true),
        domino_clipboard_enabled: true,
        ..UiConfig::default()
    };
    let panel = SettingsPanel::new(&config);
    assert!(!panel.compat.check_on_startup);
    assert!(panel.compat.warning_suppressed);
    assert!(
        panel.compat.domino_clipboard_enabled,
        "Domino 开关应镜像 UiConfig"
    );
    assert_eq!(
        panel.compat.check_state,
        lumino_ui_core::state::GpuCheckUiState::Idle
    );
}

#[test]
fn test_compat_domino_clipboard_defaults_to_disabled() {
    let panel = SettingsPanel::new(&UiConfig::default());
    assert!(
        !panel.compat.domino_clipboard_enabled,
        "Domino 剪贴板互粘开关默认应为关闭"
    );
}

#[test]
fn test_compat_events_update_switches_and_state() {
    let mut panel = panel_with_tempo(512.0);
    panel.update(Event::GpuCheckOnStartupChanged(false));
    assert!(!panel.compat.check_on_startup);
    panel.update(Event::GpuWarningSuppressedChanged(true));
    assert!(panel.compat.warning_suppressed);
    panel.update(Event::DominoClipboardEnabledChanged(true));
    assert!(panel.compat.domino_clipboard_enabled);
    panel.update(Event::DominoClipboardEnabledChanged(false));
    assert!(!panel.compat.domino_clipboard_enabled);
    panel.update(Event::RunGpuCompatibilityCheck);
    assert_eq!(
        panel.compat.check_state,
        lumino_ui_core::state::GpuCheckUiState::Idle,
        "面板不得本地置 Running：检测状态由 Runner 单点裁决后回推"
    );
    assert_eq!(
        panel.compat.retry_gate,
        lumino_ui_core::state::GpuCheckRetryGate::Ready,
        "默认门控必须为可重试"
    );
}

// ── 关于页回声洞彩蛋（UI-006）──

#[test]
fn test_default_menu_index_is_not_about() {
    // UI-006 的装填点是「菜单切到关于页」（`Event::MenuSelected(MENU_INDEX_ABOUT)`）。
    // 该设计的前提是：关于页**永远不是**设置面板的初始页——否则打字机永远不会启动。
    // 若将来有人把关于页设为默认页，这条会变红，提示必须在 `view` 路径补装填。
    let panel = SettingsPanel::new(&UiConfig::default());
    assert_ne!(
        panel.selected_menu_index, MENU_INDEX_ABOUT,
        "关于页成了初始页：回声洞打字机将永不启动，请在视图路径补装填（见 handlers/settings.rs）"
    );
    assert_eq!(panel.selected_menu_index, 0, "设置面板默认停在第一页");
}

#[test]
fn test_echo_cave_click_event_does_not_panic() {
    // 面板不持有回声洞状态（归属 `RootState`），本事件在面板侧只是穷尽匹配占位；
    // 这里守住「事件可达且不改变面板配置」这一契约。
    let mut panel = SettingsPanel::new(&UiConfig::default());
    panel.update(Event::EchoCaveClicked);
    assert_eq!(panel.selected_menu_index, 0);
}

/// `MENU_INDEX_ABOUT` 必须与菜单表**绑定**，而不是一个能独立漂移的裸字面量。
///
/// 该常量同时驱动两件事：页面选择（`lib.rs::render_content_area`）与回声洞打字机
/// 装填（`handlers/settings.rs`）。若将来在「关于」前后增删菜单项，两者会一起错位：
/// 新项占住索引 9 会在**不存在的页面**上触发打字机，而菜单里的「关于」落到
/// `render_placeholder`——**且不会有任何编译错误**。
/// 上面的 `test_default_menu_index_is_not_about` 只锁「初始页不是关于页」，
/// 锁不住这个漂移。
#[test]
fn test_menu_index_about_is_bound_to_menu_table() {
    use lumino_extras::i18n::{Language, settings_translations};

    let items = crate::menu::create_menu_items(Language::ZhCn);
    let about = settings_translations(Language::ZhCn).about;

    assert!(
        MENU_INDEX_ABOUT < items.len(),
        "MENU_INDEX_ABOUT({MENU_INDEX_ABOUT}) 越界（菜单共 {} 项）：装填点会落到不存在的页面",
        items.len()
    );
    assert_eq!(
        items[MENU_INDEX_ABOUT].0, about,
        "MENU_INDEX_ABOUT({MENU_INDEX_ABOUT}) 指向的不是「关于」页：打字机装填点已漂移"
    );
    assert_eq!(
        items.iter().filter(|(label, _)| *label == about).count(),
        1,
        "「关于」项必须唯一，否则 MENU_INDEX_ABOUT 的语义不明确"
    );
}

// ── GPU 缓冲状态指示器设置 ──

/// 面板初始化镜像 UiConfig 三个指示器字段；默认（配置默认）为开/开/3 秒平均。
#[test]
fn test_gpu_status_panel_init_and_defaults() {
    let panel = SettingsPanel::new(&UiConfig::default());
    assert!(panel.synth.gpu_buffer_status_enabled, "总开关默认开");
    assert!(
        panel.synth.gpu_governor_metrics_enabled,
        "GOV/DROP 段默认开"
    );
    assert_eq!(
        panel.synth.gpu_load_display,
        lumino_core::storage::config::GpuLoadDisplay::ShortAverage,
        "LOAD 口径默认 3 秒平均"
    );

    // 配置显式关闭 / 瞬时 → 面板镜像
    let config = UiConfig {
        gpu_buffer_status_enabled: false,
        gpu_governor_metrics_enabled: false,
        gpu_load_display: lumino_core::storage::config::GpuLoadDisplay::Instant,
        ..UiConfig::default()
    };
    let panel = SettingsPanel::new(&config);
    assert!(!panel.synth.gpu_buffer_status_enabled);
    assert!(!panel.synth.gpu_governor_metrics_enabled);
    assert_eq!(
        panel.synth.gpu_load_display,
        lumino_core::storage::config::GpuLoadDisplay::Instant
    );
}

/// 事件更新三个字段（开关随时可切换、口径可切换）。
#[test]
fn test_gpu_status_events_update_fields() {
    use lumino_core::storage::config::GpuLoadDisplay;

    let mut panel = SettingsPanel::new(&UiConfig::default());
    panel.update(Event::GpuBufferStatusEnabledChanged(false));
    assert!(!panel.synth.gpu_buffer_status_enabled);
    panel.update(Event::GpuGovernorMetricsEnabledChanged(false));
    assert!(!panel.synth.gpu_governor_metrics_enabled);
    panel.update(Event::GpuLoadDisplayChanged(GpuLoadDisplay::Instant));
    assert_eq!(panel.synth.gpu_load_display, GpuLoadDisplay::Instant);
    panel.update(Event::GpuLoadDisplayChanged(GpuLoadDisplay::ShortAverage));
    assert_eq!(panel.synth.gpu_load_display, GpuLoadDisplay::ShortAverage);
}

// ── 加载保护（内存检查 / 进程强杀看门狗，均默认关闭）──

/// 面板初始化镜像 UiConfig；全新配置两个开关都必须是关闭。
#[test]
fn test_load_guard_panel_defaults_and_mirror() {
    let panel = SettingsPanel::new(&UiConfig::default());
    assert!(
        !panel.logging.load_memory_check_enabled,
        "加载期间内存检查默认应为关闭"
    );
    assert!(
        !panel.logging.load_watchdog_enabled,
        "加载期间强杀看门狗默认应为关闭"
    );

    let config = UiConfig {
        load_memory_check_enabled: true,
        load_watchdog_enabled: true,
        ..UiConfig::default()
    };
    let panel = SettingsPanel::new(&config);
    assert!(panel.logging.load_memory_check_enabled, "应镜像配置值");
    assert_eq!(
        panel.logging.load_watchdog_enabled,
        cfg!(not(target_os = "macos")),
        "应镜像配置值；macOS 上看门狗整体禁用，必须归一化为关闭（置灰开关若显示已勾选则无法取消）"
    );
}

/// macOS 归一化契约：配置里残留 true 也不能让置灰开关显示「已勾选」。
#[test]
fn test_load_watchdog_normalized_on_macos() {
    let config = UiConfig {
        load_watchdog_enabled: true,
        ..UiConfig::default()
    };
    let panel = SettingsPanel::new(&config);
    if cfg!(target_os = "macos") {
        assert!(
            !panel.logging.load_watchdog_enabled,
            "macOS 上看门狗不启用，面板必须归一化为关闭"
        );
    } else {
        assert!(
            panel.logging.load_watchdog_enabled,
            "非 macOS 平台应镜像配置值"
        );
    }
}

/// 事件可双向切换两个开关（面板只改本地状态，持久化由 Runner 统一落盘）。
#[test]
fn test_load_guard_events_toggle_switches() {
    let mut panel = SettingsPanel::new(&UiConfig::default());
    panel.update(Event::LoadMemoryCheckEnabledChanged(true));
    assert!(panel.logging.load_memory_check_enabled);
    panel.update(Event::LoadMemoryCheckEnabledChanged(false));
    assert!(!panel.logging.load_memory_check_enabled);

    panel.update(Event::LoadWatchdogEnabledChanged(true));
    assert!(panel.logging.load_watchdog_enabled);
    panel.update(Event::LoadWatchdogEnabledChanged(false));
    assert!(!panel.logging.load_watchdog_enabled);
}
