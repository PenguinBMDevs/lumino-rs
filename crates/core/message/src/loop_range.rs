//! 循环区域动作

/// 循环区域动作
#[derive(Debug, Clone)]
pub enum LoopRangeAction {
    /// 切换循环启用/禁用
    Toggle,
    /// 设置循环范围（起始tick，结束tick）
    SetRange(f32, f32),
    /// 清除循环范围
    Clear,
    /// 标尺上鼠标按下（用于拖拽循环边界）
    RulerPressed { x: f32, y: f32 },
    /// 标尺上鼠标移动
    RulerMoved { x: f32, y: f32 },
    /// 标尺上鼠标释放
    RulerReleased,
    /// 标尺双击（切换循环）
    RulerDoubleClicked { x: f32, y: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_range_action_variants() {
        let action = LoopRangeAction::Toggle;
        assert!(matches!(action, LoopRangeAction::Toggle));

        let action = LoopRangeAction::SetRange(0.0, 100.0);
        assert!(matches!(action, LoopRangeAction::SetRange(_, _)));
    }
}
