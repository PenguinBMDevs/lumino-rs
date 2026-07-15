//! 编辑器操作 - 对话框管理

use crate::root::Root;
use crate::state::root_state::DialogType;
use crate::toolbar;

impl Root {
    /// 设置菜单打开状态（菜单打开时不渲染预览音符）
    pub fn set_menu_open(&mut self, open: bool) {
        self.state.is_menu_open = open;
    }

    /// 获取当前是否应该渲染预览音符
    pub fn should_render_preview_note(&self) -> bool {
        !self.state.is_menu_open && !self.is_progress_window
    }

    /// 更新编辑器鼠标位置
    pub fn update_editor_cursor(&mut self, position: Option<iced_core::Point>) {
        self.editor.update_cursor_position(position);
    }

    /// 更新编辑器 Canvas 偏移量
    pub fn set_editor_canvas_offset(&mut self, offset: iced_core::Point) {
        self.editor.set_canvas_offset(offset);
    }

    /// 设置自定义精度对话框是否打开
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.state.custom_precision_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::CustomPrecision;
        } else if self.state.dialog_type == DialogType::CustomPrecision {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置工程设置对话框是否打开
    pub fn set_project_settings_dialog_open(&mut self, open: bool) {
        self.state.project_settings_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::ProjectSettings;
        } else if self.state.dialog_type == DialogType::ProjectSettings {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置设置对话框是否打开
    pub fn set_settings_dialog_open(&mut self, open: bool) {
        if open {
            self.state.dialog_type = DialogType::Settings;
        } else if self.state.dialog_type == DialogType::Settings {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置音符变速对话框是否打开
    pub fn set_speed_change_dialog_open(&mut self, open: bool) {
        if open {
            self.state.dialog_type = DialogType::SpeedChange;
        } else if self.state.dialog_type == DialogType::SpeedChange {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 应用音符变速
    pub fn apply_speed_change(&mut self, factor: f32) {
        tracing::info!("应用音符变速: 倍率={}", factor);
        self.toolbar.speed_factor = factor;
        let modified = self.editor.apply_speed_change(factor);
        if modified > 0 {
            tracing::info!("变速完成，修改了 {} 个音符", modified);
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
    }

    /// 设置工程设置对话框数据
    pub fn set_project_settings_data(
        &mut self,
        title: String,
        tempo: String,
        copyright: String,
        created_display: String,
        total_editing_time_seconds: f64,
    ) {
        self.state.project_settings_dialog.title = title;
        self.state.project_settings_dialog.tempo = tempo;
        self.state.project_settings_dialog.copyright = copyright;
        self.state.project_settings_dialog.created_display = created_display;
        self.state
            .project_settings_dialog
            .total_editing_time_seconds = total_editing_time_seconds;
    }

    /// 应用工程设置到主窗口
    pub fn apply_project_settings(&mut self, title: String, tempo: f64, copyright: String) {
        tracing::info!(
            "应用工程设置: 标题={}, BPM={}, 版权={}",
            title,
            tempo,
            copyright
        );

        // 持久化标题和版权
        self.state.project_settings_dialog.title = title;
        self.state.project_settings_dialog.copyright = copyright;
        self.state.project_settings_dialog.tempo = format!("{:.0}", tempo);

        // 同步到编辑器 tempo 数据（用户编辑的源）
        self.editor.editor_state.data.tempo_points =
            vec![crate::editor::editor_state::TempoPoint {
                tick: 0.0,
                bpm: tempo,
            }];

        // 同步到播放管理器
        let tempo_micros = lumino_midi_loader::bpm_to_tempo(tempo) as u32;
        self.load_tempo_changes(vec![(0, tempo_micros)]);
    }

    /// 获取当前项目设置数据（用于填充工程设置对话框）
    /// 返回 (title, tempo, copyright, created_display, total_editing_time_seconds)
    pub fn get_project_settings_data(&self) -> (String, String, String, String, f64) {
        let dialog = &self.state.project_settings_dialog;
        // 从编辑器 tempo_points 读取当前 BPM（反映工程设置和指挥轨道编辑的变更）
        let tempo = self
            .editor
            .editor_state
            .data
            .tempo_points
            .first()
            .map(|tp| format!("{:.1}", tp.bpm))
            .unwrap_or_else(|| dialog.tempo.clone());
        let created_display = dialog.created_display.clone();
        let editing_time = dialog.total_editing_time_seconds;

        // 从 MIDI 文档获取标题和版权（如果有）
        let (title, copyright) = if let Some(_doc) = &self.midi.document {
            // 尝试从文件名获取标题
            let title = if dialog.title.is_empty() {
                // 使用默认标题
                "无标题".to_string()
            } else {
                dialog.title.clone()
            };
            (title, dialog.copyright.clone())
        } else {
            (dialog.title.clone(), dialog.copyright.clone())
        };

        (title, tempo, copyright, created_display, editing_time)
    }

    /// 设置加载确认对话框（使用文件路径和大小）
    pub fn set_load_confirm_dialog(&mut self, file_path: &str, size_mb: f64) {
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());
        self.state.load_confirm_dialog = crate::state::root_state::LoadConfirmDialogState {
            is_open: true,
            file_name,
            file_path: file_path.to_string(),
            size_mb,
        };
        self.state.dialog_type = crate::state::root_state::DialogType::LoadConfirm;
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<crate::host::DialogResult> {
        self.state.dialog_result.take()
    }

    /// 应用设置面板配置到主窗口（只同步修改过的配置）
    pub fn apply_settings(&mut self, new_settings: crate::settings::SettingsPanel) {
        let old_settings = self.settings.clone();

        tracing::info!("apply_settings: 开始同步设置到主窗口");

        // 同步主题（主题存储在 window.theme 中，不在 SettingsPanel 中）
        // 主题需要通过 dialog_result 传递
        // 注意：主题同步由 process_dialog_result 中的 settings_dialog_theme 处理

        // 只同步修改过的配置项
        if old_settings.eraser_behavior != new_settings.eraser_behavior {
            tracing::info!(
                "同步橡皮擦行为: {:?} -> {:?}",
                old_settings.eraser_behavior,
                new_settings.eraser_behavior
            );
            self.editor
                .set_eraser_behavior(new_settings.eraser_behavior);
        }

        if old_settings.selection_box_mode != new_settings.selection_box_mode {
            tracing::info!(
                "同步框选框模式: {:?} -> {:?}",
                old_settings.selection_box_mode,
                new_settings.selection_box_mode
            );
            self.editor
                .set_selection_box_mode(new_settings.selection_box_mode);
        }

        if old_settings.velocity_filter_threshold != new_settings.velocity_filter_threshold {
            tracing::info!(
                "同步力度过滤阈值: {} -> {}",
                old_settings.velocity_filter_threshold,
                new_settings.velocity_filter_threshold
            );
            self.visual.velocity_filter_threshold = new_settings.velocity_filter_threshold;
            // 阈值变化会改变哪些音符应当发声，需要重建播放队列
            self.update_playback_notes();
        }

        // 同步自动滚动配置（只同步修改过的项）
        let mut auto_scroll_changed = false;
        let mut auto_scroll_config = *self.editor.auto_scroll_config();

        if old_settings.auto_scroll_fixed_position != new_settings.auto_scroll_fixed_position {
            tracing::info!(
                "同步自动滚动固定位置: {} -> {}",
                old_settings.auto_scroll_fixed_position,
                new_settings.auto_scroll_fixed_position
            );
            auto_scroll_config.fixed_indicator_position = new_settings.auto_scroll_fixed_position;
            auto_scroll_changed = true;
        }

        if old_settings.auto_scroll_page_trigger_offset
            != new_settings.auto_scroll_page_trigger_offset
        {
            tracing::info!(
                "同步自动滚动翻页触发偏移: {} -> {}",
                old_settings.auto_scroll_page_trigger_offset,
                new_settings.auto_scroll_page_trigger_offset
            );
            auto_scroll_config.page_trigger_offset = new_settings.auto_scroll_page_trigger_offset;
            auto_scroll_changed = true;
        }

        if old_settings.auto_scroll_page_return_position
            != new_settings.auto_scroll_page_return_position
        {
            tracing::info!(
                "同步自动滚动翻页返回位置: {} -> {}",
                old_settings.auto_scroll_page_return_position,
                new_settings.auto_scroll_page_return_position
            );
            auto_scroll_config.page_return_position = new_settings.auto_scroll_page_return_position;
            auto_scroll_changed = true;
        }

        if auto_scroll_changed {
            self.editor.set_auto_scroll_config(auto_scroll_config);
        }

        // 同步 HiDPI 图标设置
        if old_settings.icon_hidpi != new_settings.icon_hidpi {
            tracing::info!(
                "同步 HiDPI 图标: {} -> {}",
                old_settings.icon_hidpi,
                new_settings.icon_hidpi
            );
            crate::resources::icon::set_hidpi_enabled(new_settings.icon_hidpi);
        }

        // 同步 256 键模式
        if old_settings.enable_256key != new_settings.enable_256key {
            tracing::info!(
                "同步 256 键模式: {} -> {}",
                old_settings.enable_256key,
                new_settings.enable_256key
            );
            let new_count: u16 = if new_settings.enable_256key { 256 } else { 128 };
            self.editor.set_visible_key_count(new_count);
            self.editor.editor_state.view.key_count = new_count;
        }

        // 同步合成器后端（需要标记重新初始化）
        if old_settings.synth_backend != new_settings.synth_backend {
            tracing::info!(
                "同步合成器后端: {:?} -> {:?}",
                old_settings.synth_backend,
                new_settings.synth_backend
            );
            // 合成器后端变更需要重新初始化，标记为需要重新初始化
            // 实际重新初始化在 save_storage 中处理
        }

        // 同步音色库路径
        if old_settings.soundfont_path != new_settings.soundfont_path {
            tracing::info!(
                "同步音色库路径: '{}' -> '{}'",
                old_settings.soundfont_path,
                new_settings.soundfont_path
            );
            // 音色库路径变更需要重新初始化，标记为需要重新初始化
        }

        // 同步 XSynth 参数
        if old_settings.xsynth_buffer_ms != new_settings.xsynth_buffer_ms
            || old_settings.xsynth_sample_rate != new_settings.xsynth_sample_rate
            || old_settings.xsynth_threads != new_settings.xsynth_threads
            || old_settings.xsynth_fade_out != new_settings.xsynth_fade_out
            || old_settings.xsynth_max_voices_per_key != new_settings.xsynth_max_voices_per_key
        {
            tracing::info!(
                "同步 XSynth 参数: buffer={:.1}ms-> {:.1}ms, threads={}-> {}, fade={}-> {}, voices={:?}-> {:?}",
                old_settings.xsynth_buffer_ms,
                new_settings.xsynth_buffer_ms,
                old_settings.xsynth_threads,
                new_settings.xsynth_threads,
                old_settings.xsynth_fade_out,
                new_settings.xsynth_fade_out,
                old_settings.xsynth_max_voices_per_key,
                new_settings.xsynth_max_voices_per_key
            );
            // XSynth 参数变更需要重新初始化
        }

        // 同步播放键盘颜色指示开关
        if old_settings.playback_key_colors_enabled != new_settings.playback_key_colors_enabled {
            tracing::info!(
                "同步播放键盘颜色: {} -> {}",
                old_settings.playback_key_colors_enabled,
                new_settings.playback_key_colors_enabled
            );
            self.editor
                .set_playback_key_colors_enabled(new_settings.playback_key_colors_enabled);
        }

        // 同步 MIDI 输入设备选择
        if old_settings.selected_midi_device != new_settings.selected_midi_device {
            tracing::info!(
                "同步 MIDI 输入设备: {:?} -> {:?}",
                old_settings.selected_midi_device,
                new_settings.selected_midi_device
            );
            // MIDI 设备选择变更需要重新打开设备
        }

        // 更新设置面板
        self.settings = new_settings;
        tracing::info!("apply_settings: 设置同步完成");
    }

    /// 设置自定义精度值
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.editor.set_snap_precision(ticks);
        self.editor.set_default_note_length(ticks);
        self.state.note_precision = toolbar::NotePrecision::Custom;
        tracing::info!("自定义精度已设置为 {} ticks", ticks);
    }

    /// 设置导出进度对话框是否打开
    pub fn set_export_progress_dialog_open(&mut self, open: bool) {
        self.state.export_progress_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::ExportProgress;
        } else if self.state.dialog_type == DialogType::ExportProgress {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置内存监控对话框是否打开
    pub fn set_memory_monitor_dialog_open(&mut self, open: bool) {
        self.state.memory_monitor_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::MemoryMonitor;
        } else if self.state.dialog_type == DialogType::MemoryMonitor {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 更新导出进度（重定向到音频导出面板内嵌进度条）
    pub fn update_export_progress(&mut self, message: String, progress: f64) {
        self.state.audio_export_dialog.render_message = message;
        self.state.audio_export_dialog.render_progress = progress;
        if progress >= 1.0 {
            self.state.audio_export_dialog.is_rendering = false;
            self.state.audio_export_dialog.render_completed = true;
        }
    }
}

#[cfg(test)]
mod dialog_tests;
