//! 贴图瀑布流视口参数
//!
//! 从宿主渲染参数（如 gfx 的 RenderParams）中提取的字段子集，
//! 避免本 crate 反向依赖宿主的渲染参数结构。

/// 贴图瀑布流视口参数
#[derive(Debug, Clone, Copy)]
pub struct WaterfallViewportParams {
    /// 物理视口大小
    pub viewport_size: (u32, u32),
    /// 缩放因子
    pub scale_factor: f32,
    /// 滚动位置 (x, y)
    pub scroll: (f32, f32),
    /// 缩放 (x, y)
    pub zoom: (f32, f32),
    /// 键盘宽度
    pub keyboard_width: f32,
    /// 标尺高度
    pub ruler_height: f32,
    /// Canvas 偏移
    pub canvas_offset: (f32, f32),
    /// Canvas 大小
    pub canvas_size: (f32, f32),
    /// 是否为音轨总览模式（总览模式下不渲染贴图瀑布流）
    pub is_arrangement_mode: bool,
}
