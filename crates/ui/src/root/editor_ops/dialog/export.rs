//! 导出对话框和加载确认对话框管理

use crate::root::Root;

impl Root {
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
