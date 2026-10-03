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
        lumino_ui_core::state::GpuCheckUiState::Running
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
