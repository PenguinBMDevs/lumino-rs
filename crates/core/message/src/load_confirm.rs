//! 加载确认对话框动作

/// 加载确认对话框动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadConfirmAction {
    /// 确认加载
    Confirm,
    /// 关闭对话框（取消加载）
    CloseDialog,
}
