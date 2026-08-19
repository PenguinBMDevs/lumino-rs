//! 洋葱皮/渲染状态子模块
//!
//! 由 Root 持有，存储与渲染相关的状态。
//! Root 字段为 `pub(crate)`，确保内边界清晰且不被外部修改。

/// 洋葱皮渲染状态（由 Root 持有）
#[derive(Debug)]
pub struct VisualState {
    /// 力度过滤阈值
    pub velocity_filter_threshold: u8,
    /// 力度面板高度，用于绘制
    pub velocity_panel_height: f32,
}

impl VisualState {
    /// 创建一个视觉渲染状态（洋葱皮）
    pub fn new(velocity_filter_threshold: u8, velocity_panel_height: f32) -> Self {
        Self {
            velocity_filter_threshold,
            velocity_panel_height,
        }
    }
}
