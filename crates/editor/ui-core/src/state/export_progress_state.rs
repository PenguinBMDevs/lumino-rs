//! 音频导出进度对话框状态

/// 音频导出进度对话框状态
#[derive(Debug, Clone)]
pub struct ExportProgressDialogState {
    /// 是否显示
    pub is_open: bool,
    /// 当前进度消息
    pub message: String,
    /// 进度值 (0.0 - 1.0)
    pub progress: f64,
    /// 是否已完成
    pub is_completed: bool,
    /// 是否出错
    pub error: Option<String>,
}

impl ExportProgressDialogState {
    /// 创建一个默认的导出进度对话框状态
    pub fn new() -> Self {
        Self {
            is_open: false,
            message: String::new(),
            progress: 0.0,
            is_completed: false,
            error: None,
        }
    }

    /// 重置状态
    pub fn reset(&mut self) {
        self.is_open = false;
        self.message.clear();
        self.progress = 0.0;
        self.is_completed = false;
        self.error = None;
    }

    /// 更新进度
    pub fn update_progress(&mut self, message: String, progress: f64) {
        self.message = message;
        self.progress = progress;
    }

    /// 标记完成
    pub fn set_completed(&mut self) {
        self.is_completed = true;
        self.progress = 1.0;
        self.message = "导出完成".to_string();
    }

    /// 标记错误
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error.clone());
        self.message = format!("导出失败: {}", error);
    }
}

impl Default for ExportProgressDialogState {
    fn default() -> Self {
        Self::new()
    }
}
