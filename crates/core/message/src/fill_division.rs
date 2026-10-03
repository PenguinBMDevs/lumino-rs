//! 颜料桶「分音符填充」对话框动作
//!
//! 填充桶的切分档位设置：画布 Ctrl+单击 → 主窗口覆盖层弹窗 → 输入 x
//! （x 分音符）→ 应用到 `LineToolState::fill_division`。
//!
//! 与独立窗口对话框（`dialog::Event`）不同：本对话框是主窗口内嵌覆盖层，
//! 无需 runner / DialogManager，直接在 Root 内闭环。

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
