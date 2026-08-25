//! 配置持久化管理
//!
//! RunnerInner 中与配置保存、差异检测等相关的实现。

use super::super::window_manager::WindowManager;
use super::*;

struct ConfigDiff {
    synth_changed: bool,
    xsynth_changed: bool,
    audio_engine_changed: bool,
    core_buffer_changed: bool,
    titlebar_changed: bool,
    font_changed: bool,
}

fn display_or_empty(s: &str) -> &str {
    if s.is_empty() { "(空)" } else { s }
}

impl RunnerInner {
    fn config_diff(
        new: &lumino_ui::settings::SettingsPanel,
        old: &lumino_core::storage::config::UiConfig,
        current_theme: &str,
    ) -> Option<ConfigDiff> {
        let theme_changed = current_theme != old.theme;
        let synth_changed = new.synth.backend != old.preferred_backend
            || new.synth.soundfont_path != old.soundfont_path;
        let xsynth_changed = new.synth.xsynth_buffer_ms != old.xsynth_buffer_ms
            || new.synth.xsynth_sample_rate != old.xsynth_sample_rate
            || new.synth.xsynth_threads != old.xsynth_threads
            || new.synth.xsynth_fade_out != old.xsynth_fade_out_killing
            || new.synth.xsynth_max_voices_per_key != old.xsynth_max_voices_per_key;
        let audio_engine_changed = new.synth.audio_engine != old.audio_engine;
        let core_buffer_changed = new.synth.core_buffer_frames != old.core_buffer_frames;
        let titlebar_changed = new.synth.use_native_titlebar != old.use_native_titlebar;
        let font_changed = new.editing.program_font_name != old.program_font_name
            || new.editing.program_font_path != old.program_font_path;
        let other_changed = new.display.language != old.language
            || new.editing.selection_box_mode != old.selection_box_mode
            || new.midi.velocity_filter_threshold != old.velocity_filter_threshold
            || new.editing.eraser_behavior != old.eraser_behavior
            || new.auto_scroll.fixed_position != old.auto_scroll.fixed_indicator_position
            || new.auto_scroll.page_trigger_offset != old.auto_scroll.page_trigger_offset
            || new.auto_scroll.page_return_position != old.auto_scroll.page_return_position
            || new.display.icon_hidpi != old.icon_hidpi
            || new.display.enable_256key != old.enable_256key
            || new.display.velocity_curve_style != old.velocity_curve_style
            || new.display.playback_key_colors_enabled != old.playback_key_colors_enabled
            || new.editing.track_add_behavior != old.track_add_behavior
            || new.editing.history_total_limit != old.history_total_limit
            || new.editing.history_entry_limit != old.history_entry_limit
            || new.editing.merge_window_ms != old.merge_window_ms
            || new.editing.intercept_notification_enabled != old.intercept_notification_enabled
            || new.editing.automation_line_thickness != old.automation_line_thickness
            || new.editing.tempo_max_bpm != old.tempo_max_bpm
            || new.logging.monitor_refresh_interval_ms != old.monitor_refresh_interval_ms;
        if theme_changed
            || synth_changed
            || xsynth_changed
            || audio_engine_changed
            || core_buffer_changed
            || titlebar_changed
            || font_changed
            || other_changed
        {
            Some(ConfigDiff {
                synth_changed,
                xsynth_changed,
                audio_engine_changed,
                core_buffer_changed,
                titlebar_changed,
                font_changed,
            })
        } else {
            None
        }
    }

    pub(crate) fn compute_custom_precision_ticks(
        ppq: f32,
        numerator: f32,
        denominator: f32,
    ) -> f32 {
        ppq * 4.0 * numerator / denominator
    }

    fn auto_scroll_mode_changed(
        window: &WindowManager,
        old_mode: lumino_core::storage::config::AutoScrollMode,
    ) -> bool {
        window.ui().root().editor.editor_state.auto_scroll.mode != old_mode
    }

    pub(crate) fn save_storage(&mut self) {
        let new = self.window_state.window.ui().settings();
        let old = &self.window_state.storage.config.get().ui;
        let current_theme = self.window_state.window.ui().root().theme().to_string();
        let auto_scroll_mode_changed =
            Self::auto_scroll_mode_changed(&self.window_state.window, old.auto_scroll.mode);
        let diff = match Self::config_diff(new, old, &current_theme) {
            Some(d) => d,
            None if !auto_scroll_mode_changed => return,
            None => ConfigDiff {
                synth_changed: false,
                xsynth_changed: false,
                audio_engine_changed: false,
                core_buffer_changed: false,
                titlebar_changed: false,
                font_changed: false,
            },
        };
        if diff.synth_changed {
            tracing::info!(
                "合成器设置已改变: backend {} -> {}, soundfont {} -> {}",
                old.preferred_backend,
                new.synth.backend,
                display_or_empty(&old.soundfont_path),
                display_or_empty(&new.synth.soundfont_path),
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.xsynth_changed {
            tracing::info!(
                "XSynth 参数已改变: buffer={:.1}ms-> {:.1}ms, sr={}-> {}, threads={}-> {}, fade={}-> {}, voices={:?}-> {:?}",
                old.xsynth_buffer_ms,
                new.synth.xsynth_buffer_ms,
                old.xsynth_sample_rate,
                new.synth.xsynth_sample_rate,
                old.xsynth_threads,
                new.synth.xsynth_threads,
                old.xsynth_fade_out_killing,
                new.synth.xsynth_fade_out,
                old.xsynth_max_voices_per_key,
                new.synth.xsynth_max_voices_per_key,
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.audio_engine_changed {
            tracing::info!(
                "音频引擎已改变: {:?} -> {:?}",
                old.audio_engine,
                new.synth.audio_engine
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.core_buffer_changed {
            tracing::info!(
                "Core 缓冲已改变: {} -> {}",
                old.core_buffer_frames,
                new.synth.core_buffer_frames
            );
            self.midi_state.midi.mark_for_reinit();
        }
        if diff.titlebar_changed {
            tracing::info!(
                "标题栏设置已改变: native_titlebar {} -> {}",
                old.use_native_titlebar,
                new.synth.use_native_titlebar
            );
            self.window_state.needs_window_restart = true;
        }
        if diff.font_changed {
            tracing::info!(
                "字体设置已改变: font_name {} -> {}, font_path {} -> {}",
                display_or_empty(&old.program_font_name),
                display_or_empty(&new.editing.program_font_name),
                display_or_empty(&old.program_font_path),
                display_or_empty(&new.editing.program_font_path),
            );
            self.window_state.needs_window_restart = true;
        }
        if current_theme != old.theme {
            tracing::info!("主题已改变: {} -> {}", old.theme, current_theme);
        }
        self.window_state.storage.config.patch(|config| {
            config.ui.theme.clone_from(&current_theme);
            config.ui.language = new.display.language;
            config.ui.preferred_backend = new.synth.backend;
            config.ui.audio_engine = new.synth.audio_engine;
            config.ui.core_buffer_frames = new.synth.core_buffer_frames;
            config.ui.soundfont_path = new.synth.soundfont_path.clone();
            config.ui.use_native_titlebar = new.synth.use_native_titlebar;
            config.ui.program_font_name = new.editing.program_font_name.clone();
            config.ui.program_font_path = new.editing.program_font_path.clone();
            config.ui.selection_box_mode = new.editing.selection_box_mode;
            config.ui.xsynth_buffer_ms = new.synth.xsynth_buffer_ms;
            config.ui.xsynth_sample_rate = new.synth.xsynth_sample_rate;
            config.ui.xsynth_threads = new.synth.xsynth_threads;
            config.ui.xsynth_fade_out_killing = new.synth.xsynth_fade_out;
            config.ui.xsynth_max_voices_per_key = new.synth.xsynth_max_voices_per_key;
            config.ui.velocity_filter_threshold = new.midi.velocity_filter_threshold;
            config.ui.eraser_behavior = new.editing.eraser_behavior;
            config.ui.auto_scroll.mode = self
                .window_state
                .window
                .ui()
                .root()
                .editor
                .editor_state
                .auto_scroll
                .mode;
            config.ui.auto_scroll.fixed_indicator_position = new.auto_scroll.fixed_position;
            config.ui.auto_scroll.page_trigger_offset = new.auto_scroll.page_trigger_offset;
            config.ui.auto_scroll.page_return_position = new.auto_scroll.page_return_position;
            config.ui.icon_hidpi = new.display.icon_hidpi;
            config.ui.enable_256key = new.display.enable_256key;
            config.ui.velocity_curve_style = new.display.velocity_curve_style;
            config.ui.hires_onion_enabled = new.hires.onion_enabled;
            config.ui.hires_measures_per_group = new.hires.measures_per_group;
            config.ui.hires_tile_width_px = new.hires.tile_width_px;
            config.ui.hires_cooldown_secs = new.hires.cooldown_secs;
            config.ui.hires_gpu_mem_limit_mb = new.hires.gpu_mem_limit_mb;
            config.ui.playback_key_colors_enabled = new.display.playback_key_colors_enabled;
            config.ui.track_add_behavior = new.editing.track_add_behavior;
            config.ui.selected_palette = new.display.selected_palette.clone();
            config.ui.history_total_limit = new.editing.history_total_limit;
            config.ui.history_entry_limit = new.editing.history_entry_limit;
            config.ui.merge_window_ms = new.editing.merge_window_ms;
            config.ui.intercept_notification_enabled = new.editing.intercept_notification_enabled;
            config.ui.automation_line_thickness = new.editing.automation_line_thickness;
            config.ui.tempo_max_bpm = new.editing.tempo_max_bpm;
            config.ui.monitor_refresh_interval_ms = new.logging.monitor_refresh_interval_ms;
            config.ui.log_retention_count = new.logging.log_retention_count;
        });
        lumino_extras::palette::set_current_palette_by_name(&new.display.selected_palette);
        if let Err(e) = self.window_state.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.window_state.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }
}
