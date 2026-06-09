//! 力度/CC/Tempo Canvas 状态定义

use std::collections::HashMap;

/// 力度 Canvas 状态
#[derive(Debug, Default)]
pub struct VelocityCanvasState {
    /// 当前正在拖拽的点索引（在 points 中的索引，非 note_index）
    pub drag_point_idx: Option<usize>,
    /// 拖拽开始时的力度值（用于 undo）
    pub _drag_start_velocity: u8,
    /// 当前悬停的点索引
    pub hover_point_idx: Option<usize>,
    /// Canvas 是否已初始化尺寸
    pub _initialized: bool,
    /// 是否在拖拽 resize 手柄
    pub resize_dragging: bool,
    /// resize 拖拽起始 Y 坐标（绝对屏幕坐标）
    pub resize_drag_start_y: f32,
    /// resize 拖拽开始时的面板高度
    pub resize_start_height: f32,
    /// 鼠标是否悬停在 resize 手柄区域
    pub hover_resize_handle: bool,
    /// 是否正在曲线绘制模式
    pub curve_active: bool,
    /// 曲线绘制起始 X 坐标（local）
    pub curve_start_x: f32,
    /// 曲线绘制起始 Y 对应的力度值
    pub curve_start_velocity: u8,
    /// 当前笔触影响的音符索引 → 新力度值
    pub curve_affected: HashMap<usize, u8>,
}
