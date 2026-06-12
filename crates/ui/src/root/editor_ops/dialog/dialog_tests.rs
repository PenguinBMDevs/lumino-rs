use super::*;
use lumino_core::storage::config::UiConfig;

#[test]
fn test_apply_settings_eraser_behavior_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.eraser_behavior = lumino_core::storage::config::EraserBehavior::DirectSelect;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.eraser_behavior, new_settings.eraser_behavior);
}

#[test]
fn test_apply_settings_selection_box_mode_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.selection_box_mode = lumino_core::storage::config::SelectionBoxMode::Spring;

    root.apply_settings(new_settings.clone());

    assert_eq!(
        root.settings.selection_box_mode,
        new_settings.selection_box_mode
    );
}

#[test]
fn test_apply_settings_velocity_filter_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.velocity_filter_threshold = 64;

    root.apply_settings(new_settings.clone());

    assert_eq!(
        root.settings.velocity_filter_threshold,
        new_settings.velocity_filter_threshold
    );
}

#[test]
fn test_apply_settings_auto_scroll_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.auto_scroll_fixed_position = 100;
    new_settings.auto_scroll_page_trigger_offset = 200;
    new_settings.auto_scroll_page_return_position = 50;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.auto_scroll_fixed_position, 100);
    assert_eq!(root.settings.auto_scroll_page_trigger_offset, 200);
    assert_eq!(root.settings.auto_scroll_page_return_position, 50);
}

#[test]
fn test_apply_settings_icon_hidpi_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.icon_hidpi = !old_settings.icon_hidpi;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.icon_hidpi, new_settings.icon_hidpi);
}

#[test]
fn test_apply_settings_256key_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.enable_256key = !old_settings.enable_256key;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.enable_256key, new_settings.enable_256key);
}

#[test]
fn test_apply_settings_no_changes() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();
    let new_settings = old_settings.clone();

    // 没有变化时，应该不触发同步
    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.eraser_behavior, old_settings.eraser_behavior);
    assert_eq!(
        root.settings.selection_box_mode,
        old_settings.selection_box_mode
    );
}

#[test]
fn test_apply_settings_synth_backend_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.synth_backend = lumino_core::storage::config::SynthBackend::Kdmapi;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.synth_backend, new_settings.synth_backend);
}

#[test]
fn test_apply_settings_soundfont_path_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.soundfont_path = "/path/to/soundfont.sf2".to_string();

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.soundfont_path, new_settings.soundfont_path);
}

#[test]
fn test_apply_settings_xsynth_buffer_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.xsynth_buffer_ms = 50.0;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.xsynth_buffer_ms, 50.0);
}

#[test]
fn test_apply_settings_xsynth_threads_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.xsynth_threads = 4;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.xsynth_threads, 4);
}

#[test]
fn test_apply_settings_xsynth_fade_out_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.xsynth_fade_out = !old_settings.xsynth_fade_out;

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.xsynth_fade_out, new_settings.xsynth_fade_out);
}

#[test]
fn test_apply_settings_xsynth_max_voices_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.xsynth_max_voices_per_key = Some(32);

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.xsynth_max_voices_per_key, Some(32));
}

#[test]
fn test_apply_settings_midi_device_changed() {
    let mut root = create_test_root();
    let old_settings = root.settings.clone();

    let mut new_settings = old_settings.clone();
    new_settings.selected_midi_device = Some(1);

    root.apply_settings(new_settings.clone());

    assert_eq!(root.settings.selected_midi_device, Some(1));
}

fn create_test_root() -> Root {
    let ui_config = UiConfig::default();
    Root::new(&ui_config)
}
