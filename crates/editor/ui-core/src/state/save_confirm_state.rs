//! 保存确认对话框状态

/// 保存确认对话框状态（关闭工程 / 打开另一个工程 / 退出前的未保存更改确认）
#[derive(Debug, Clone, Default)]
pub struct SaveConfirmDialogState {
    /// 对话框是否打开
    pub is_open: bool,
}
