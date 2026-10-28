//! 撤销历史面板动作（DRAW-035）

/// 历史面板动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    /// 跳到历史游标（0 = 初始状态）
    Jump(usize),
    /// 关闭历史面板
    Close,
}
