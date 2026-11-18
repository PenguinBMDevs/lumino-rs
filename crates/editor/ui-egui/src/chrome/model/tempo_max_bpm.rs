//! 自定义 Tempo BPM 上限对话框模型（DRAW-045，独立 OS 窗口）。
//!
//! 输入串与错误提示都来自生产侧对话框状态；egui 只做排版与命中。

/// 「自定义 BPM 上限」对话框的只读模型。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TempoMaxBpmModel {
    /// 顶部是否有 30px 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题（「自定义 BPM 上限」）。
    pub title: String,
    /// 输入行标签（「Tempo BPM 上限:」）。
    pub label: String,
    /// 当前输入串。
    pub input: String,
    /// 说明文案（上限的作用）。
    pub hint: String,
    /// 确认失败原因（`None` = 不显示错误行）。
    pub error: Option<String>,
    /// 确定按钮文案。
    pub ok: String,
    /// 取消按钮文案。
    pub cancel: String,
}
