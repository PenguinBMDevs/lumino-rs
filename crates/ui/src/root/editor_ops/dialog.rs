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
        }
    }

    /// 设置工程设置对话框是否打开
    pub fn set_project_settings_dialog_open(&mut self, open: bool) {
        self.state.project_settings_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::ProjectSettings;
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

        // 同步到播放管理器
        let tempo_micros = lumino_core::bpm_to_tempo(tempo) as u32;
        self.load_tempo_changes(vec![(0, tempo_micros)]);
    }

    /// 获取当前项目设置数据（用于填充工程设置对话框）
    /// 返回 (title, tempo, copyright, created_display, total_editing_time_seconds)
    pub fn get_project_settings_data(&self) -> (String, String, String, String, f64) {
        let dialog = &self.state.project_settings_dialog;
        let tempo = dialog.tempo.clone();
        let created_display = dialog.created_display.clone();
        let editing_time = dialog.total_editing_time_seconds;

        // 从 MIDI 文档获取标题和版权（如果有）
        let (title, copyright) = if let Some(_doc) = &self.midi_document {
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
        let old_settings = &self.settings;

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
            self.velocity_filter_threshold = new_settings.velocity_filter_threshold;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsPanel;
    use lumino_core::storage::config::UiConfig;

    fn create_test_settings() -> SettingsPanel {
        let ui_config = UiConfig::default();
        SettingsPanel::new(&ui_config)
    }

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

    fn create_test_root() -> Root {
        let ui_config = UiConfig::default();
        Root::new(&ui_config)
    }
}
