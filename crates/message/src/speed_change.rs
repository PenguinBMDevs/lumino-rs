//! 音符变速动作

/// 音符变速动作
#[derive(Debug, Clone)]
pub enum SpeedChangeAction {
    /// 打开音符变速对话框
    OpenDialog,
    /// 关闭音符变速对话框
    CloseDialog,
    /// 确认音符变速
    Confirm,
    /// 速度倍率输入变更
    FactorChanged(String),
}
