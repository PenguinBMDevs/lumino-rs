//! 「音符画设置」对话框动作
//!
//! 与 `brush_settings` 同构：主窗侧 `OpenDialog` 触发 Runner 打开**独立 OS 窗口**
//! （各绘制工具的公共参数聚合入口，见 `view/draw_settings_dialog.rs`）。
//!
//! 之所以不复用 `brush_settings` 的通道：两者是**不同窗口类型**
//! （`DialogType::BrushSettings` / `DialogType::DrawSettings`），
//! 共用动作枚举会让"画刷绘制行为"与"音符画设置"在事件层无法区分，
//! 沦为靠 payload 猜意图。

/// 「音符画设置」对话框动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawSettingsAction {
    /// 打开对话框（主窗侧，由悬浮条右端齿轮按钮触发）
    OpenDialog,
    /// 关闭对话框（对话框内「关闭」按钮）
    CloseDialog,
}
