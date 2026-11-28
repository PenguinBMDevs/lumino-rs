//! 云浏览面板的动作与单帧通道。
//!
//! 动作只回传意图（下标 / 路径 / 字段文本），磁盘与网络操作仍由 Runner 完成。

const ACTION_ID: &str = "lumino.cloud-browser.action";

/// 云浏览面板产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudBrowserAction {
    /// 切换设备（`devices` 下标）。
    SelectDevice(usize),
    /// 刷新当前目录。
    Refresh,
    /// 断开当前设备。
    Disconnect,
    /// 粘贴剪贴板内容。
    Paste,
    /// 清空剪贴板。
    ClearClipboard,
    /// 返回上级目录。
    Back,
    /// 进入目录。
    EnterDir(String),
    /// 新建文件夹输入变化。
    NewFolderInputChanged(String),
    /// 新建文件夹。
    NewFolder,
    /// 保存到当前目录。
    SaveHere,
    /// 下载文件。
    Download(String),
    /// 复制条目。
    Copy {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 剪切条目。
    Cut {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 开始重命名。
    StartRename(String),
    /// 请求删除（进入确认态）。
    RequestDelete {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 重命名输入变化。
    RenameInputChanged(String),
    /// 确认重命名。
    RenameConfirm,
    /// 取消重命名。
    RenameCancel,
    /// 确认删除。
    DeleteEntry {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 取消删除。
    DeleteCancel,
    /// 关闭断连提醒。
    DismissNotice,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: CloudBrowserAction) {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).push_first(ctx, action);
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<CloudBrowserAction> {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).take(ctx)
}
