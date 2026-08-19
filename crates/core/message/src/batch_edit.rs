//! 批量编辑动作

/// 批量编辑对话框中的目标字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchEditField {
    /// 音符力度
    Velocity,
    /// 音符长度
    Gate,
    /// 音符 key 位置
    Key,
    /// 音符 tick 位置
    Tick,
}

/// 批量编辑动作
#[derive(Debug, Clone)]
pub enum BatchEditAction {
    /// 打开批量编辑对话框
    OpenDialog,
    /// 关闭批量编辑对话框
    CloseDialog,
    /// 确认批量编辑
    Confirm,
    /// 字段输入变更
    InputChanged(BatchEditField, String),
}
