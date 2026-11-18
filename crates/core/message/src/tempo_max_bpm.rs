//! 自定义 Tempo BPM 上限对话框动作（DRAW-045，独立 OS 窗口）。
//!
//! 输入串与校验结果留在对话框自己的状态里；本枚举只承载用户操作，
//! 校验通过后的 `bpm` 由生产侧 `DialogResult` 回传。

/// 「自定义 BPM 上限」对话框动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempoMaxBpmDialogAction {
    /// 输入框文本变化（原样回传，校验在确认时进行）。
    Input(String),
    /// 确认：生产侧解析并应用。
    Confirm,
    /// 取消（按钮或窗口 X）。
    Cancel,
}
