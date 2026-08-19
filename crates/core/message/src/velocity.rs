//! 力度 / CC / Tempo 编辑面板动作

/// 力度 / CC / Tempo 编辑面板动作
#[derive(Debug, Clone)]
pub enum VelocityAction {
    /// 力度拖拽开始：需要 push history 进行撤销支持
    /// 参数: (note_index, velocity)
    DragStart(usize, u8),
    /// 力度拖拽移动中：仅更新力度，不 push history
    /// 参数: (note_index, new_velocity)
    DragMove(usize, u8),
    /// 力度拖拽结束
    DragEnd,
    /// 力度曲线绘制开始：push history 保存绘制前状态
    CurveStart,
    /// 力度曲线绘制更新：批量应用力度变化，不 push history
    /// 参数: Vec<(note_index, u8)>
    CurvePaint(Vec<(usize, u8)>),
    /// 力度曲线绘制结束
    CurveEnd,
    /// 切换编辑模式（力度/Tempo/CC/Bend）
    ToggleMode,
    /// 选择 CC 控制器编号
    CcControllerSelected(u8),
    /// 选择 CC 或 Bend 选项
    CcOptionSelected(crate::CcOption),
    /// 速度编辑：拖拽开始
    /// 参数: (point_index 在 data.tempo_points 中的索引)
    TempoDragStart(usize),
    /// 速度编辑：拖拽移动中，更新 BPM
    /// 参数: (point_index, new_bpm)
    TempoDragMove(usize, f64),
    /// 速度编辑：拖拽结束
    TempoDragEnd,
    /// 速度编辑：添加速度点 (tick, bpm)
    TempoAdd(f32, f64),
    /// 速度编辑：删除速度点
    TempoDelete(usize),
    /// 自动化编辑：先 push history 再应用单个编辑。
    /// 用于单击、双击、右键删除等瞬时操作。
    AutomationEdit(lumino_note_core::AutomationEdit),
    /// 自动化批量编辑：不 push history，用于拖拽/曲线绘制中的连续更新。
    AutomationBatch(Vec<lumino_note_core::AutomationEdit>),
    /// 自动化拖拽开始：push history，不应用编辑。
    AutomationDragStart,
    /// 调整自动化曲线垂直缩放。
    /// 参数: 缩放因子（相乘），在 0.01..8.0 之间钳制。
    AutomationZoom(f32),
    /// 双向滚轮滚动：水平分量滚动时间轴（X 轴），垂直分量滚动自动化曲线。
    /// 单条消息携带双轴分量，支持「同时向上+向左」等对角线滚动。
    /// 参数: (水平增量, 垂直增量)，单位为滚轮档位（Pixels 源 ÷50 换算）。
    WheelScrolled {
        /// 水平滚动增量（X 轴）
        delta_x: f32,
        /// 垂直滚动增量（自动化曲线）
        delta_y: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_action_variants() {
        let action = VelocityAction::DragStart(0, 100);
        assert!(matches!(action, VelocityAction::DragStart(_, _)));

        let action = VelocityAction::ToggleMode;
        assert!(matches!(action, VelocityAction::ToggleMode));

        let action = VelocityAction::TempoAdd(0.0, 120.0);
        assert!(matches!(action, VelocityAction::TempoAdd(_, _)));
    }
}
