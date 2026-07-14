//! 加载确认对话框状态

/// 加载确认对话框状态
#[derive(Debug, Clone)]
pub struct LoadConfirmDialogState {
    pub is_open: bool,
    pub file_name: String,
    pub file_path: String,
    pub size_mb: f64,
}

impl Default for LoadConfirmDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            file_name: String::new(),
            file_path: String::new(),
            size_mb: 0.0,
        }
    }
}
