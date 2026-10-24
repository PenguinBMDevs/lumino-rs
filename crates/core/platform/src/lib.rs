//! Lumino 平台能力门面。
//!
//! 存在的意义只有一个：**把「桌面才有」的能力收口到一处**，让上层 UI / runner 代码
//! 不直接依赖 `rfd` / `arboard`——这两个 crate 都没有 Android 后端，直接依赖会导致
//! 整个 workspace 在 `--target aarch64-linux-android` 下编不过（且报错位置在依赖内部，
//! 排查成本高）。
//!
//! 当前覆盖：
//! - [`dialog`]：文件选择 / 保存 / 目录选择 / 消息框；
//! - [`clipboard`]：文本剪贴板读写。
//!
//! Android 侧现状（L1 里程碑）：只保证**能编译 + 行为可预期**——
//! 文件对话框返回 `None`（等同用户取消）、消息框返回 `Ok`、剪贴板返回 `Unavailable`。
//! 真正的 SAF / ClipboardManager 接入排在 L3/L4（届时只需改本 crate 的两个实现文件，
//! 上层 29 处调用点零改动）。

pub mod clipboard;
pub mod dialog;

pub use clipboard::{ClipboardError, get_text, set_text};
pub use dialog::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
