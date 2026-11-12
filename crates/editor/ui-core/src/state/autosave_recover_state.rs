//! 自动保存恢复对话框状态（DEBT-12）
//!
//! 只在启动阶段使用：Runner 检测到上次会话遗留的自动保存后，把候选路径与
//! 修改时间注入独立对话框窗口，由用户选择「恢复」或「不恢复」。

/// 自动保存恢复对话框状态
#[derive(Debug, Clone, Default)]
pub struct AutosaveRecoverDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 待恢复文件的完整路径（展示用）
    pub path: String,
    /// 文件修改时刻的本地化文本（取不到时为空）
    pub modified_at: String,
}
