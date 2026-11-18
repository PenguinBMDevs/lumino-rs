//! 素材删除确认对话框动作（DRAW-045，独立 OS 窗口）。
//!
//! 对话框只回传用户决策；文件删除、列表刷新与 Toast 仍由主窗口的
//! `RightSidebarAction::MaterialDeleteConfirmed` 语义完成。

/// 素材删除确认对话框动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialDeleteDialogAction {
    /// 确认删除（携带待删除素材下标）。
    Confirm(usize),
    /// 取消（按钮、遮罩外点击或窗口 X）。
    Cancel,
}
