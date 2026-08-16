//! 设置对话框动作

/// 设置对话框动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsDialogAction {
    /// 打开设置对话框
    OpenDialog,
    /// 关闭设置对话框
    CloseDialog,
}
