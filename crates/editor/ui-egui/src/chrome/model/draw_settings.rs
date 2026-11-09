//! 「音符画设置」对话框模型。

/// 音符画设置：标题 / 占位说明 / 关闭。
#[derive(Debug, Clone, Default)]
pub struct DrawSettingsModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题（与窗口标题、悬浮条齿轮 tooltip 同文案）。
    pub title: String,
    /// 占位说明（明确告知开发中）。
    pub placeholder: String,
    /// 关闭按钮。
    pub close: String,
}
