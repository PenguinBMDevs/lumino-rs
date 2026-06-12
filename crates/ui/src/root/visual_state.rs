//! 视觉/渲染状态管理
//!
//! 从 Root 中提取的洋葱皮缓存、力度面板等与显示相关的状态，
//! 减少 Root 的字段数并明确生命周期边界。

use iced_core::Color;

/// 视觉渲染状态（从 Root 提取）
#[derive(Debug)]
pub struct VisualState {
    /// 洋葱皮音符原始数据缓存（tick, key, length, color）
    /// 存原始数据而非 NoteInstance，因为 NoteInstance 含屏幕坐标（随 scroll/zoom 变化）
    pub(crate) cached_onion_skin_notes: Option<Vec<(f32, u16, f32, Color)>>,
    /// 缓存失效计数器（只有音轨数据/开关变化才递增）
    pub(crate) onion_skin_generation: u64,
    /// 力度过滤阈值
    pub(crate) velocity_filter_threshold: u8,
    /// 力度面板高度（可拖拽调整）
    pub(crate) velocity_panel_height: f32,
}

impl VisualState {
    pub fn new(velocity_filter_threshold: u8, velocity_panel_height: f32) -> Self {
        Self {
            cached_onion_skin_notes: None,
            onion_skin_generation: 0,
            velocity_filter_threshold,
            velocity_panel_height,
        }
    }
}
