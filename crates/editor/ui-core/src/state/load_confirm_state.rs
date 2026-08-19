//! 加载确认对话框状态

/// 加载确认对话框状态
#[derive(Debug, Clone)]
pub struct LoadConfirmDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 文件名
    pub file_name: String,
    /// 文件路径
    pub file_path: String,
    /// 文件大小（MB）
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
