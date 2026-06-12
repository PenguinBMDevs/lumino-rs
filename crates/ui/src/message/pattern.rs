//! Pattern 编辑动作（音轨总览中的音符片段）

/// Pattern 编辑动作
#[derive(Debug, Clone)]
pub enum PatternAction {
    /// 选中 Pattern
    Selected(u32),
    /// 左边缘拖拽开始（参数: pattern_id）
    DragStartLeft(u32),
    /// 右边缘拖拽开始（参数: pattern_id）
    DragStartRight(u32),
    /// 左边缘拖拽移动中（参数: pattern_id, new_start_tick）
    DragMoveLeft(u32, f32),
    /// 右边缘拖拽移动中（参数: pattern_id, new_length）
    DragMoveRight(u32, f32),
    /// 拖拽结束
    DragEnd,
}
