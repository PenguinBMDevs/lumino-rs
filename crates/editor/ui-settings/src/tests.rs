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
        ..UiConfig::default()
    };
    let panel = SettingsPanel::new(&config);
    assert!(!panel.compat.check_on_startup);
    assert!(panel.compat.warning_suppressed);
    assert_eq!(
        panel.compat.check_state,
        lumino_ui_core::state::GpuCheckUiState::Idle
    );
}

#[test]
fn test_compat_events_update_switches_and_state() {
    let mut panel = panel_with_tempo(512.0);
    panel.update(Event::GpuCheckOnStartupChanged(false));
    assert!(!panel.compat.check_on_startup);
    panel.update(Event::GpuWarningSuppressedChanged(true));
    assert!(panel.compat.warning_suppressed);
    panel.update(Event::RunGpuCompatibilityCheck);
    assert_eq!(
        panel.compat.check_state,
        lumino_ui_core::state::GpuCheckUiState::Running
    );
}
