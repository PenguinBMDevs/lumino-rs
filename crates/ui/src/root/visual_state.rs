//! 视觉/渲染状态管理
//!
//! 从 Root 中提取的力度面板等与显示相关的状态，
//! 减少 Root 的字段数并明确生命周期边界。

/// 视觉渲染状态（从 Root 提取）
#[derive(Debug)]
pub struct VisualState {
    /// 力度过滤阈值
    pub(crate) velocity_filter_threshold: u8,
    /// 力度面板高度（可拖拽调整）
    pub(crate) velocity_panel_height: f32,
}

impl VisualState {
    pub fn new(velocity_filter_threshold: u8, velocity_panel_height: f32) -> Self {
        Self {
            velocity_filter_threshold,
            velocity_panel_height,
        }
    }
}
