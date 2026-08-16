use super::*;
use lumino_core::storage::config::UiConfig;

#[test]
fn test_apply_settings_eraser_behavior_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.editing.eraser_behavior = lumino_core::storage::config::EraserBehavior::DirectSelect;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.editing.eraser_behavior, new_settings.editing.eraser_behavior);
}

#[test]
fn test_apply_settings_selection_box_mode_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.editing.selection_box_mode = lumino_core::storage::config::SelectionBoxMode::Spring;

    root.apply_settings(new_settings.clone());

    assert_eq!(
        root.settings.editing.selection_box_mode,
        new_settings.editing.selection_box_mode
    );
}

#[test]
fn test_apply_settings_velocity_filter_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.midi.velocity_filter_threshold = 64;

    root.apply_settings(new_settings.clone());

    assert_eq!(
        root.settings.midi.velocity_filter_threshold,
        new_settings.midi.velocity_filter_threshold
    );
}

#[test]
fn test_apply_settings_auto_scroll_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.auto_scroll.fixed_position = 100;
    new_settings.auto_scroll.page_trigger_offset = 200;
    new_settings.auto_scroll.page_return_position = 50;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.auto_scroll.fixed_position, 100);
    assert_eq!(root.settings.auto_scroll.page_trigger_offset, 200);
    assert_eq!(root.settings.auto_scroll.page_return_position, 50);
}

#[test]
fn test_apply_settings_icon_hidpi_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.display.icon_hidpi = !old_settings.display.icon_hidpi;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.display.icon_hidpi, new_settings.display.icon_hidpi);
}

#[test]
fn test_apply_settings_256key_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.display.enable_256key = !old_settings.display.enable_256key;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.display.enable_256key, new_settings.display.enable_256key);
}

#[test]
fn test_apply_settings_no_changes() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();
    let new_settings = old_settings.clone();

    // 没有变化时，应该不触发同步
    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.editing.eraser_behavior, old_settings.editing.eraser_behavior);
    assert_eq!(
        root.settings.editing.selection_box_mode,
        old_settings.editing.selection_box_mode
    );
}

#[test]
fn test_apply_settings_synth_backend_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.synth.backend = lumino_core::storage::config::SynthBackend::Kdmapi;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.synth.backend, new_settings.synth.backend);
}

#[test]
fn test_apply_settings_soundfont_path_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.synth.soundfont_path = "/path/to/soundfont.sf2".to_string();

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.synth.soundfont_path, new_settings.synth.soundfont_path);
}

#[test]
fn test_apply_settings_xsynth_buffer_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.synth.xsynth_buffer_ms = 50.0;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.synth.xsynth_buffer_ms, 50.0);
}

#[test]
fn test_apply_settings_xsynth_threads_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.synth.xsynth_threads = 4;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.synth.xsynth_threads, 4);
}

#[test]
fn test_apply_settings_xsynth_fade_out_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.synth.xsynth_fade_out = !old_settings.synth.xsynth_fade_out;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.synth.xsynth_fade_out, new_settings.synth.xsynth_fade_out);
}

#[test]
fn test_apply_settings_xsynth_max_voices_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.synth.xsynth_max_voices_per_key = Some(32);

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.synth.xsynth_max_voices_per_key, Some(32));
}

#[test]
fn test_apply_settings_midi_device_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.midi.selected_device = Some(1);

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.midi.selected_device, Some(1));
}

#[test]
fn test_apply_settings_automation_line_thickness_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.editing.automation_line_thickness = 5.5;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.editing.automation_line_thickness, 5.5);
    assert_eq!(root.editor.velocity_panel.automation_line_thickness, 5.5);
}

#[test]
fn test_apply_settings_tempo_max_bpm_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.editing.tempo_max_bpm = 1024.0;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.editing.tempo_max_bpm, 1024.0);
    assert_eq!(root.editor.velocity_panel.tempo_max_bpm, 1024.0);
}

#[test]
fn test_apply_settings_tempo_max_bpm_unchanged_keeps_default() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    root.apply_settings(old_settings.clone());

    // 未修改时保持默认 512
    assert_eq!(root.editor.velocity_panel.tempo_max_bpm, 512.0);
}

fn create_test_root() -> Root {
    let ui_config = UiConfig::default();
    Root::new(&ui_config)
}
