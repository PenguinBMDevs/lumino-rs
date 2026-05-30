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

    /// 设置自定义精度值
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.editor.set_snap_precision(ticks);
        self.editor.set_default_note_length(ticks);
        self.state.note_precision = toolbar::NotePrecision::Custom;
        tracing::info!("自定义精度已设置为 {} ticks", ticks);
    }
}
