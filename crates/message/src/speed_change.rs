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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speed_change_action_variants() {
        let action = SpeedChangeAction::OpenDialog;
        assert!(matches!(action, SpeedChangeAction::OpenDialog));

        let action = SpeedChangeAction::FactorChanged("0.5".to_string());
        assert!(matches!(action, SpeedChangeAction::FactorChanged(_)));
    }
}
