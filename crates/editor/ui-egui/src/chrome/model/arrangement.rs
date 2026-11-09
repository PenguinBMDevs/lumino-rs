//! 工程走带视图的 egui 模型（标尺起点）。
//!
//! 只做数据搬运：几何量（scroll / zoom / ppq / 拍号）来自生产
//! `arrangement_view.viewport` 与 `editor_state`，绘制在
//! `crate::chrome::arrangement`。

/// 走带标尺参数。
#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementRulerModel {
    /// 水平滚动偏移（像素）。
    pub scroll_x: f32,
    /// 每 tick 的像素数。
    pub zoom_x: f32,
    /// 每四分音符 tick 数。
    pub ppq: u16,
    /// 走带内容区左缘（逻辑点，相对窗口）：左侧栏总宽。
    pub left_inset: f32,
    /// 走带内容区顶缘（逻辑点，相对窗口）：标题栏 + 实际工具栏高度。
    pub top_inset: f32,
    /// 拍号变化表（原样传入；绘制侧负责同 tick 去重）。
    pub time_signatures: Vec<(u32, u8, u8)>,
}

impl Default for ArrangementRulerModel {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            zoom_x: 0.1,
            ppq: 960,
            left_inset: 48.0,
            top_inset: 102.0,
            time_signatures: vec![(0, 4, 4)],
        }
    }
}
