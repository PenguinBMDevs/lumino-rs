use bytemuck::{Pod, Zeroable};

/// 每张贴图的 uniform（32 字节，满足 16 字节对齐）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct HiResUniform {
    /// area 矩形在 framebuffer 中的 X（左上角）
    pub area_x: f32,
    /// area 矩形在 framebuffer 中的 Y（左上角）
    pub area_y: f32,
    /// area 矩形宽度
    pub area_w: f32,
    /// area 矩形高度
    pub area_h: f32,
    /// canvas 总宽度（像素）
    pub canvas_w: f32,
    /// canvas 总高度（像素）
    pub canvas_h: f32,
    _pad0: f32,
    _pad1: f32,
}

impl HiResUniform {
    pub fn new(
        area_x: f32,
        area_y: f32,
        area_w: f32,
        area_h: f32,
        canvas_w: f32,
        canvas_h: f32,
    ) -> Self {
        Self {
            area_x,
            area_y,
            area_w,
            area_h,
            canvas_w,
            canvas_h,
            _pad0: 0.0,
            _pad1: 0.0,
        }
    }
}
