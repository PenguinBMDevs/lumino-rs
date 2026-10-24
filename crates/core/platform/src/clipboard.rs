//! 文本剪贴板门面。
//!
//! 桌面走 `arboard`；Android 走占位实现（`Unavailable`），真正的
//! `ClipboardManager` + `ClipData` JNI 接入随 SAF 一起在 L3/L4 完成
//! （`MainActivity` 里已经预留了 `readClipboard` / `writeClipboard` 两个 JNI 出口）。

/// 剪贴板错误
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// 当前平台/环境没有可用剪贴板（如 Android 未接入、无显示服务的 Linux）
    #[error("剪贴板不可用: {0}")]
    Unavailable(String),
    /// 后端调用失败（权限、选区格式不支持等）
    #[error("剪贴板操作失败: {0}")]
    Backend(String),
}

/// 读取剪贴板文本
pub fn get_text() -> Result<String, ClipboardError> {
    imp::get_text()
}

/// 写入剪贴板文本
pub fn set_text(text: impl Into<String>) -> Result<(), ClipboardError> {
    imp::set_text(text.into())
}

// ── 桌面实现（arboard）────────────────────────────────────────

#[cfg(not(target_os = "android"))]
mod imp {
    use super::ClipboardError;

    fn open() -> Result<arboard::Clipboard, ClipboardError> {
        arboard::Clipboard::new().map_err(|error| ClipboardError::Unavailable(error.to_string()))
    }

    pub(super) fn get_text() -> Result<String, ClipboardError> {
        open()?
            .get_text()
            .map_err(|error| ClipboardError::Backend(error.to_string()))
    }

    pub(super) fn set_text(text: String) -> Result<(), ClipboardError> {
        open()?
            .set_text(text)
            .map_err(|error| ClipboardError::Backend(error.to_string()))
    }
}

// ── Android 占位实现 ─────────────────────────────────────────

#[cfg(target_os = "android")]
mod imp {
    use super::ClipboardError;

    pub(super) fn get_text() -> Result<String, ClipboardError> {
        Err(ClipboardError::Unavailable(
            "Android 剪贴板尚未接入（待 L3/L4 走 JNI ClipboardManager）".to_string(),
        ))
    }

    pub(super) fn set_text(_text: String) -> Result<(), ClipboardError> {
        tracing::warn!(
            "平台能力未接入（Android）：写剪贴板 → 丢弃本次内容。\
             待 L3/L4 走 JNI ClipboardManager。"
        );
        Err(ClipboardError::Unavailable(
            "Android 剪贴板尚未接入（待 L3/L4 走 JNI ClipboardManager）".to_string(),
        ))
    }
}
