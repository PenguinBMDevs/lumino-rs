//! 被保存确认对话框挂起的关闭类动作。
//!
//! 独立成文件：`inner.rs` 已超过 600 行红线，本枚举是自包含的数据类型，
//! 从主文件拆出可避免继续膨胀。

/// 被保存确认对话框挂起的关闭类动作
///
/// 工程存在未保存更改时，这些动作不会立即执行，而是先弹确认对话框，
/// 待用户决策后再继续。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingCloseAction {
    /// 关闭当前工程（菜单「关闭」）
    CloseProject,
    /// 新建工程（菜单「新建」）
    NewProject,
    /// 打开另一个工程（菜单「打开」）
    OpenProject,
    /// 打开工程文件夹（第一屏「打开工程文件夹」）
    OpenProjectFolder,
    /// 打开「最近打开」子菜单中的文件（保存确认后继续）
    OpenRecent(std::path::PathBuf),
    /// 退出软件（菜单「退出」）
    Exit,
    /// 点击窗口关闭按钮（X / 自制标题栏关闭）
    WindowClose,
}
