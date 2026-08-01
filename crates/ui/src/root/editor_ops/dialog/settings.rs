//! 设置面板配置同步到主窗口

use crate::root::Root;
use crate::settings::SettingsPanel;

impl Root {
    /// 应用设置面板配置到主窗口（只同步修改过的配置）
    pub fn apply_settings(&mut self, new_settings: SettingsPanel) {
        let old_settings = self.settings.clone();

        tracing::info!("apply_settings: 开始同步设置到主窗口");

        // 同步主题（主题存储在 window.theme 中，不在 SettingsPanel 中）
        // 主题需要通过 dialog_result 传递
        // 注意：主题同步由 process_dialog_result 中的 settings_dialog_theme 处理

        self.sync_editor_interaction_settings(&old_settings, &new_settings);
        self.sync_auto_scroll_settings(&old_settings, &new_settings);
        self.sync_display_settings(&old_settings, &new_settings);
        self.sync_audio_settings(&old_settings, &new_settings);
        self.sync_midi_device_settings(&old_settings, &new_settings);

        // 更新设置面板
        self.settings = new_settings;
        // 同步编辑历史配置到 Editor history（让 UiConfig 4 字段实时生效）
        self.sync_history_config();
        tracing::info!("apply_settings: 设置同步完成");
    }

    /// 同步编辑历史配置（max_size / merge_window_ms / max_entries_per_group）到 Editor history
    ///
    /// 在 `apply_settings` 末尾调用，让 UiConfig 的 4 个编辑字段实时生效。
    pub fn sync_history_config(&mut self) {
        let settings = &self.settings;
        self.editor.editor_state.data.history.set_config(
            settings.history_total_limit,
            settings.merge_window_ms,
            settings.history_entry_limit as u32,
        );
    }

    /// 是否允许显示编辑拦截 Toast（由 UiConfig 控制）
    pub fn intercept_notification_enabled(&self) -> bool {
        self.settings.intercept_notification_enabled
    }

    // ─── 私有辅助方法 ───────────────────────────────────────

    /// 同步编辑器交互设置（橡皮擦、框选、力度过滤、播放键盘颜色、自动化连线粗细）
    fn sync_editor_interaction_settings(&mut self, old: &SettingsPanel, new: &SettingsPanel) {
        if old.eraser_behavior != new.eraser_behavior {
            tracing::info!(
                "同步橡皮擦行为: {:?} -> {:?}",
                old.eraser_behavior,
                new.eraser_behavior
            );
            self.editor.set_eraser_behavior(new.eraser_behavior);
        }

        if old.selection_box_mode != new.selection_box_mode {
            tracing::info!(
                "同步框选框模式: {:?} -> {:?}",
                old.selection_box_mode,
                new.selection_box_mode
            );
            self.editor.set_selection_box_mode(new.selection_box_mode);
        }

        if old.velocity_filter_threshold != new.velocity_filter_threshold {
            tracing::info!(
                "同步力度过滤阈值: {} -> {}",
                old.velocity_filter_threshold,
                new.velocity_filter_threshold
            );
            self.visual.velocity_filter_threshold = new.velocity_filter_threshold;
            // 阈值变化会改变哪些音符应当发声，需要重建播放队列
            self.update_playback_notes();
        }

        if old.playback_key_colors_enabled != new.playback_key_colors_enabled {
            tracing::info!(
                "同步播放键盘颜色: {} -> {}",
                old.playback_key_colors_enabled,
                new.playback_key_colors_enabled
            );
            self.editor
                .set_playback_key_colors_enabled(new.playback_key_colors_enabled);
        }

        if old.automation_line_thickness != new.automation_line_thickness {
            tracing::info!(
                "同步自动化曲线连线粗细: {} -> {}",
                old.automation_line_thickness,
                new.automation_line_thickness
            );
            self.editor.velocity_panel.automation_line_thickness = new.automation_line_thickness;
        }
    }

    /// 同步自动滚动配置
    fn sync_auto_scroll_settings(&mut self, old: &SettingsPanel, new: &SettingsPanel) {
        let mut changed = false;
        let mut config = *self.editor.auto_scroll_config();

        if old.auto_scroll_fixed_position != new.auto_scroll_fixed_position {
            tracing::info!(
                "同步自动滚动固定位置: {} -> {}",
                old.auto_scroll_fixed_position,
                new.auto_scroll_fixed_position
            );
            config.fixed_indicator_position = new.auto_scroll_fixed_position;
            changed = true;
        }

        if old.auto_scroll_page_trigger_offset != new.auto_scroll_page_trigger_offset {
            tracing::info!(
                "同步自动滚动翻页触发偏移: {} -> {}",
                old.auto_scroll_page_trigger_offset,
                new.auto_scroll_page_trigger_offset
            );
            config.page_trigger_offset = new.auto_scroll_page_trigger_offset;
            changed = true;
        }

        if old.auto_scroll_page_return_position != new.auto_scroll_page_return_position {
            tracing::info!(
                "同步自动滚动翻页返回位置: {} -> {}",
                old.auto_scroll_page_return_position,
                new.auto_scroll_page_return_position
            );
            config.page_return_position = new.auto_scroll_page_return_position;
            changed = true;
        }

        if changed {
            self.editor.set_auto_scroll_config(config);
        }
    }

    /// 同步显示设置（HiDPI 图标、256 键模式、音轨列表显示模式）
    fn sync_display_settings(&mut self, old: &SettingsPanel, new: &SettingsPanel) {
        if old.icon_hidpi != new.icon_hidpi {
            tracing::info!("同步 HiDPI 图标: {} -> {}", old.icon_hidpi, new.icon_hidpi);
            crate::resources::icon::set_hidpi_enabled(new.icon_hidpi);
        }

        if old.enable_256key != new.enable_256key {
            tracing::info!(
                "同步 256 键模式: {} -> {}",
                old.enable_256key,
                new.enable_256key
            );
            let new_count: u16 = if new.enable_256key { 256 } else { 128 };
            self.editor.set_visible_key_count(new_count);
            self.editor.editor_state.view.key_count = new_count;
        }

        if old.track_display_mode != new.track_display_mode {
            tracing::info!(
                "同步音轨列表显示模式: {:?} -> {:?}",
                old.track_display_mode,
                new.track_display_mode
            );
            self.sidebar.track_display_mode = new.track_display_mode;
            self.sidebar.reapply_display_mode();
        }
    }

    /// 同步音频相关设置（合成器后端、音色库路径、XSynth 参数）
    fn sync_audio_settings(&mut self, old: &SettingsPanel, new: &SettingsPanel) {
        if old.synth_backend != new.synth_backend {
            tracing::info!(
                "同步合成器后端: {:?} -> {:?}",
                old.synth_backend,
                new.synth_backend
            );
            // 合成器后端变更需要重新初始化，标记为需要重新初始化
            // 实际重新初始化在 save_storage 中处理
        }

        if old.soundfont_path != new.soundfont_path {
            tracing::info!(
                "同步音色库路径: '{}' -> '{}'",
                old.soundfont_path,
                new.soundfont_path
            );
            // 音色库路径变更需要重新初始化，标记为需要重新初始化
        }

        if old.xsynth_buffer_ms != new.xsynth_buffer_ms
            || old.xsynth_sample_rate != new.xsynth_sample_rate
            || old.xsynth_threads != new.xsynth_threads
            || old.xsynth_fade_out != new.xsynth_fade_out
            || old.xsynth_max_voices_per_key != new.xsynth_max_voices_per_key
        {
            tracing::info!(
                "同步 XSynth 参数: buffer={:.1}ms-> {:.1}ms, threads={}-> {}, fade={}-> {}, voices={:?}-> {:?}",
                old.xsynth_buffer_ms,
                new.xsynth_buffer_ms,
                old.xsynth_threads,
                new.xsynth_threads,
                old.xsynth_fade_out,
                new.xsynth_fade_out,
                old.xsynth_max_voices_per_key,
                new.xsynth_max_voices_per_key
            );
            // XSynth 参数变更需要重新初始化
        }
    }

    /// 同步 MIDI 输入设备选择
    fn sync_midi_device_settings(&mut self, old: &SettingsPanel, new: &SettingsPanel) {
        if old.selected_midi_device != new.selected_midi_device {
            tracing::info!(
                "同步 MIDI 输入设备: {:?} -> {:?}",
                old.selected_midi_device,
                new.selected_midi_device
            );
            // MIDI 设备选择变更需要重新打开设备
        }
    }
}
