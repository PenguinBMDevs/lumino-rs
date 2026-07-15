//! 内存监控对话框状态

/// 内存监控独立对话框状态
#[derive(Debug, Clone, Default)]
pub struct MemoryMonitorDialogState {
    /// 对话框是否处于打开状态
    pub is_open: bool,
}

impl MemoryMonitorDialogState {
    /// 创建默认关闭状态的对话框状态
    pub fn new() -> Self {
        Self { is_open: false }
    }
}
