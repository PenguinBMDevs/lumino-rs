//! 弯音编辑器动作

use lumino_core::{BendDrawMode, PitchBendAnchor};

/// 弯音编辑器动作
#[derive(Debug, Clone)]
pub enum PitchBendAction {
    /// 进入弯音编辑模式
    /// 参数: 基准音符的 MIDI key
    Enter(u16),
    /// 退出弯音编辑模式（触发提交写入）
    Exit,
    /// 创建锚点
    CreateAnchor(PitchBendAnchor),
    /// 移动锚点
    /// 参数: (锚点索引, 新 tick, 新 value)
    MoveAnchor(usize, u32, i16),
    /// 删除锚点
    /// 参数: 锚点索引
    DeleteAnchor(usize),
    /// 选中锚点
    /// 参数: 锚点索引（None 取消选中）
    SelectAnchor(Option<usize>),
    /// 拖拽控制柄
    /// 参数: (锚点索引, handle_out_x, handle_out_y, handle_in_x, handle_in_y)
    DragHandle(usize, f32, f32, f32, f32),
    /// 切换绘制模式（曲线/直线）
    SetDrawMode(BendDrawMode),
    /// 切换对称/非对称模式
    /// 参数: 锚点索引
    ToggleSymmetry(usize),
    /// 设置基准音符
    /// 参数: MIDI key
    SetBaseKey(u16),
}
