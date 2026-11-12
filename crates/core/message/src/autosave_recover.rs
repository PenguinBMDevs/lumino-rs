//! 自动保存恢复对话框动作（DEBT-12）
//!
//! 启动时若检测到上次会话遗留的自动保存，走应用内对话框让用户选择：
//! 恢复为未保存工程，或放弃恢复（文件改名保留，下次不再询问）。

/// 自动保存恢复对话框动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosaveRecoverAction {
    /// 恢复自动保存内容（按未保存工程载入）
    Restore,
    /// 不恢复（文件改名保留）
    Discard,
}
