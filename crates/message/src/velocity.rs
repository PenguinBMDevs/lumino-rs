//! 力度 / Tempo 编辑面板动作

/// 力度 / Tempo 编辑面板动作
///
/// CC/Bend 控制器编辑与自动化曲线编辑已从 UI 移除（数据层保留）。
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
    /// 切换编辑模式（力度/Tempo）
    ToggleMode,
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
}
