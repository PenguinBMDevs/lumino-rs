//! 文件对话框 / 消息框门面。
//!
//! API 形状刻意对齐 `rfd`（调用方几乎只需把 `rfd::X` 换成
//! `lumino_platform::dialog::X`），但只暴露本仓实际用到的能力——多出来的 builder
//! 方法只会增加 Android 占位的维护面。

use std::path::{Path, PathBuf};

/// 文件对话框（选择 / 保存 / 选目录）
#[derive(Debug, Clone, Default)]
pub struct FileDialog {
    filters: Vec<(String, Vec<String>)>,
    title: Option<String>,
    directory: Option<PathBuf>,
    file_name: Option<String>,
}

impl FileDialog {
    /// 新建文件对话框
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加扩展名过滤器（`extensions` 不含点，例如 `["mid", "midi"]`）
    ///
    /// `name` 接受 `&str` 与 `String`（与 rfd 一致，调用方无需为动态标签改写法）。
    pub fn add_filter(mut self, name: impl Into<String>, extensions: &[&str]) -> Self {
        self.filters.push((
            name.into(),
            extensions.iter().map(|ext| (*ext).to_string()).collect(),
        ));
        self
    }

    /// 设置标题
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 设置初始目录
    pub fn set_directory(mut self, directory: impl AsRef<Path>) -> Self {
        self.directory = Some(directory.as_ref().to_path_buf());
        self
    }

    /// 设置默认文件名（保存对话框用）
    pub fn set_file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    /// 选择单个文件；取消返回 `None`
    pub fn pick_file(self) -> Option<PathBuf> {
        imp::pick_file(&self)
    }

    /// 选择保存路径；取消返回 `None`
    pub fn save_file(self) -> Option<PathBuf> {
        imp::save_file(&self)
    }

    /// 选择目录；取消返回 `None`
    pub fn pick_folder(self) -> Option<PathBuf> {
        imp::pick_folder(&self)
    }
}

/// 消息框图标级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    /// 信息
    Info,
    /// 警告
    Warning,
    /// 错误
    Error,
}

/// 消息框按钮组合
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageButtons {
    /// 仅「确定」
    Ok,
    /// 「确定 / 取消」
    OkCancel,
    /// 自定义两个按钮文案（确定、取消）
    OkCancelCustom(String, String),
}

/// 消息框返回值
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageDialogResult {
    /// 确定
    Ok,
    /// 取消
    Cancel,
    /// 自定义按钮（携带按钮文案）
    Custom(String),
}

/// 消息框
#[derive(Debug, Clone, Default)]
pub struct MessageDialog {
    title: Option<String>,
    description: Option<String>,
    level: Option<MessageLevel>,
    buttons: Option<MessageButtons>,
}

impl MessageDialog {
    /// 新建消息框
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置标题
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 设置正文
    pub fn set_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 设置图标级别
    pub fn set_level(mut self, level: MessageLevel) -> Self {
        self.level = Some(level);
        self
    }

    /// 设置按钮组合
    pub fn set_buttons(mut self, buttons: MessageButtons) -> Self {
        self.buttons = Some(buttons);
        self
    }

    /// 显示并阻塞等待结果
    pub fn show(self) -> MessageDialogResult {
        imp::show_message(&self)
    }
}

// ── 桌面实现（rfd）────────────────────────────────────────────

#[cfg(not(target_os = "android"))]
mod imp {
    use std::path::PathBuf;

    use super::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

    /// 把门面选项转成 rfd 的 builder
    fn build(dialog: &FileDialog) -> rfd::FileDialog {
        let mut builder = rfd::FileDialog::new();

        for (name, extensions) in &dialog.filters {
            let refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
            builder = builder.add_filter(name, &refs);
        }
        if let Some(title) = &dialog.title {
            builder = builder.set_title(title);
        }
        if let Some(directory) = &dialog.directory {
            builder = builder.set_directory(directory);
        }
        if let Some(file_name) = &dialog.file_name {
            builder = builder.set_file_name(file_name);
        }

        builder
    }

    pub(super) fn pick_file(dialog: &FileDialog) -> Option<PathBuf> {
        build(dialog).pick_file()
    }

    pub(super) fn save_file(dialog: &FileDialog) -> Option<PathBuf> {
        build(dialog).save_file()
    }

    pub(super) fn pick_folder(dialog: &FileDialog) -> Option<PathBuf> {
        build(dialog).pick_folder()
    }

    pub(super) fn show_message(dialog: &MessageDialog) -> MessageDialogResult {
        let mut builder = rfd::MessageDialog::new();

        if let Some(title) = &dialog.title {
            builder = builder.set_title(title);
        }
        if let Some(description) = &dialog.description {
            builder = builder.set_description(description);
        }
        if let Some(level) = dialog.level {
            builder = builder.set_level(match level {
                MessageLevel::Info => rfd::MessageLevel::Info,
                MessageLevel::Warning => rfd::MessageLevel::Warning,
                MessageLevel::Error => rfd::MessageLevel::Error,
            });
        }
        if let Some(buttons) = &dialog.buttons {
            builder = builder.set_buttons(match buttons {
                MessageButtons::Ok => rfd::MessageButtons::Ok,
                MessageButtons::OkCancel => rfd::MessageButtons::OkCancel,
                MessageButtons::OkCancelCustom(ok, cancel) => {
                    rfd::MessageButtons::OkCancelCustom(ok.clone(), cancel.clone())
                }
            });
        }

        match builder.show() {
            rfd::MessageDialogResult::Ok => MessageDialogResult::Ok,
            rfd::MessageDialogResult::Cancel => MessageDialogResult::Cancel,
            rfd::MessageDialogResult::Custom(label) => MessageDialogResult::Custom(label),
            // rfd 的其余按钮形态（YesNo / OkCustom 等）本仓未使用，统一按「确定」处理
            _ => MessageDialogResult::Ok,
        }
    }
}

// ── Android 占位实现 ─────────────────────────────────────────
//
// 不静默失败：每次都记 warn，方便在 logcat 里直接看出「这个功能还没接」，
// 而不是让用户以为是权限/路径问题。

#[cfg(target_os = "android")]
mod imp {
    use std::path::PathBuf;

    use super::{FileDialog, MessageDialog, MessageDialogResult};

    pub(super) fn pick_file(dialog: &FileDialog) -> Option<PathBuf> {
        warn_unsupported("选择文件", dialog.title.as_deref());
        None
    }

    pub(super) fn save_file(dialog: &FileDialog) -> Option<PathBuf> {
        warn_unsupported("选择保存路径", dialog.title.as_deref());
        None
    }

    pub(super) fn pick_folder(dialog: &FileDialog) -> Option<PathBuf> {
        warn_unsupported("选择目录", dialog.title.as_deref());
        None
    }

    pub(super) fn show_message(dialog: &MessageDialog) -> MessageDialogResult {
        tracing::warn!(
            "平台能力未接入（Android）：消息框「{}」 —— {}；按「确定」返回",
            dialog.title.as_deref().unwrap_or("<无标题>"),
            dialog.description.as_deref().unwrap_or("<无正文>")
        );
        // 一律返回「确定」：对「警告后继续」「信息提示」是安全默认；
        // 对 OkCancel 型确认（如设备门控）Android 侧本来就走不到该分支。
        MessageDialogResult::Ok
    }

    fn warn_unsupported(action: &str, title: Option<&str>) {
        tracing::warn!(
            "平台能力未接入（Android）：{action}（标题「{}」）→ 按「用户取消」处理。\
             待 L3/L4 接 Storage Access Framework。",
            title.unwrap_or("<无标题>")
        );
    }
}
