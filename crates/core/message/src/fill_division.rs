//! 颜料桶「分音符填充」面板动作
//!
//! 填充桶的切分档位设置：画布 Ctrl+单击 / 音符画工具箱 Ctrl+单击颜料桶 →
//! 工具栏上方的小面板 → 输入 x（x 分音符）→ 应用到 `LineToolState::fill_division`。
//!
//! 该面板与画刷 / 形状下拉同属「工具栏 Ctrl+弹出的小面板」家族，由
//! `state.fill_division_dialog`（is_open + value）驱动，无需 runner / DialogManager。

/// 分音符填充对话框动作
#[derive(Debug, Clone)]
pub enum FillDivisionAction {
    /// 打开对话框（画布 Ctrl+单击触发）
    OpenDialog,
    /// 关闭对话框（取消 / 点击遮罩）
    CloseDialog,
    /// 输入框内容变更（仅接受数字；空串表示恢复整块填充）
    ValueChanged(String),
    /// 确认：把输入值写入填充桶切分档位
    Confirm,
}
