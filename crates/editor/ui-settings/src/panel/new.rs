//! `SettingsPanel::new` — 设置面板的构造器
//!
//! 从 `panel.rs` 纯结构拆出（把 `panel.rs` 保持在 400 行以内），**语义零变化**：
//! 只做 `UiConfig → SettingsPanel` 的字段搬运与调色板解析，不持有分发状态。
//!
//! 事件分发（`SettingsPanel::update`）仍留在 `panel.rs`，其 `match` 保持对
//! `Event` 全变体的**编译期穷尽**，不引入 `_ => {}` 通配臂。

use crate::{
    AutoScrollSettings, CloudSettings, DisplaySettings, EditingSettings, HiresSettings,
    LoggingSettings, MidiSettings, SettingsPanel, SynthSettings,
};

impl SettingsPanel {
    /// 根据给定的 UI 配置创建设置面板状态。
    ///
    /// 从 `ui_config` 中初始化各子设置，解析并填充可用/选中的调色板名称。
    ///
    /// # 参数
    /// * `ui_config` — 应用 UI 配置来源
    ///
    /// # 返回值
    /// 返回按配置类别初始化完成的 `SettingsPanel` 实例
    pub fn new(ui_config: &lumino_core::storage::config::UiConfig) -> Self {
        let palette_mgr = &*lumino_extras::palette::PALETTE_MANAGER;
        let available_palettes = palette_mgr.names().to_vec();
        let selected_palette = if ui_config.selected_palette.is_empty() {
            palette_mgr.default().name.to_string()
        } else {
            palette_mgr
                .resolve_name(&ui_config.selected_palette)
                .to_string()
        };

        Self {
            selected_menu_index: 0,
            synth: SynthSettings {
                backend: ui_config.preferred_backend,
                audio_engine: ui_config.audio_engine,
                soundfont_path: ui_config.soundfont_path.clone(),
                use_native_titlebar: ui_config.use_native_titlebar,
                xsynth_buffer_ms: ui_config.xsynth_buffer_ms,
                xsynth_sample_rate: ui_config.xsynth_sample_rate,
                xsynth_threads: ui_config.xsynth_threads,
                xsynth_max_voices_per_key: ui_config.xsynth_max_voices_per_key,
                xsynth_global_voice_limit: ui_config.xsynth_global_voice_limit,
                xsynth_voice_target_ratio: ui_config.xsynth_voice_target_ratio,
                xsynth_soft_nps_gate: ui_config.xsynth_soft_nps_gate,
                lgs_block_size: ui_config.lgs_block_size,
                lgs_max_voices_per_key: ui_config.lgs_max_voices_per_key,
                lgs_global_voice_limit: ui_config.lgs_global_voice_limit,
                lgs_soft_nps_gate: ui_config.lgs_soft_nps_gate,
                lgs_velocity_filter_threshold: ui_config.lgs_velocity_filter_threshold,
                gpu_buffer_status_enabled: ui_config.gpu_buffer_status_enabled,
                gpu_governor_metrics_enabled: ui_config.gpu_governor_metrics_enabled,
                gpu_load_display: ui_config.gpu_load_display,
                audio_output_devices: Vec::new(),
                selected_audio_output_device: ui_config.audio_output_device.clone(),
            },
            editing: EditingSettings {
                eraser_behavior: ui_config.eraser_behavior,
                selection_box_mode: ui_config.selection_box_mode,
                program_font_name: ui_config.program_font_name.clone(),
                program_font_path: ui_config.program_font_path.clone(),
                history_total_limit: ui_config.history_total_limit,
                history_entry_limit: ui_config.history_entry_limit,
                merge_window_ms: ui_config.merge_window_ms,
                intercept_notification_enabled: ui_config.intercept_notification_enabled,
                automation_line_thickness: ui_config.automation_line_thickness,
                tempo_max_bpm: ui_config.tempo_max_bpm,
                tempo_custom_open: false,
                tempo_custom_input: String::new(),
                track_add_behavior: ui_config.track_add_behavior,
            },
            display: DisplaySettings {
                icon_hidpi: ui_config.icon_hidpi,
                enable_256key: ui_config.enable_256key,
                note_count_display: ui_config.note_count_display,
                velocity_curve_style: ui_config.velocity_curve_style,
                language: ui_config.language,
                language_preference: ui_config.effective_language_preference(),
                playback_key_colors_enabled: ui_config.playback_key_colors_enabled,
                selected_palette,
                available_palettes,
            },
            auto_scroll: AutoScrollSettings {
                fixed_position: ui_config.auto_scroll.fixed_indicator_position,
                page_trigger_offset: ui_config.auto_scroll.page_trigger_offset,
                page_return_position: ui_config.auto_scroll.page_return_position,
            },
            midi: MidiSettings {
                devices: Vec::new(),
                selected_device: None,
                velocity_filter_threshold: ui_config.velocity_filter_threshold,
                winmm_outputs: Vec::new(),
                selected_winmm_output: ui_config.system_output_device_id,
            },
            hires: HiresSettings {
                onion_enabled: ui_config.hires_onion_enabled,
                measures_per_group: ui_config.hires_measures_per_group,
                tile_width_px: ui_config.hires_tile_width_px,
                cooldown_secs: ui_config.hires_cooldown_secs,
                gpu_mem_limit_mb: ui_config.hires_gpu_mem_limit_mb,
                backend: ui_config.onion_skin_backend,
                // 会话态：由主窗口通过 `set_settings_onion_backend_locked` 注入，
                // 面板自身永远从「未锁定」起步（无 MIDI 时新建面板即可切换）。
                backend_locked: false,
            },
            logging: LoggingSettings {
                log_retention_count: ui_config.log_retention_count,
                monitor_refresh_interval_ms: ui_config.monitor_refresh_interval_ms,
            },
            cloud: CloudSettings {
                connections: Vec::new(),
                alert: None,
            },
            compat: crate::CompatSettings {
                check_on_startup: ui_config.gpu_check_on_startup,
                warning_suppressed: ui_config.gpu_warning_suppressed.unwrap_or(false),
                domino_clipboard_enabled: ui_config.domino_clipboard_enabled,
                check_state: lumino_ui_core::state::GpuCheckUiState::Idle,
                retry_gate: lumino_ui_core::state::GpuCheckRetryGate::Ready,
                copied: false,
            },
        }
    }
}
