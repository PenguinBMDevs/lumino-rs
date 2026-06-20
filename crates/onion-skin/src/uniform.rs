//! 洋葱皮渲染器 Uniform 类型

/// 洋葱皮着色器 Uniform 数据（与 WGSL 中的 struct Uniform 对应）
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionSkinUniform {
    /// 卷帘区域在 framebuffer 中的 X 位置
    pub area_x: f32,
    /// 卷帘区域在 framebuffer 中的 Y 位置
    pub area_y: f32,
    /// 卷帘区域宽度
    pub area_w: f32,
    /// 卷帘区域高度
    pub area_h: f32,
    /// 当前视口可见的起始时间（毫秒）
    pub time_start_ms: f32,
    /// 当前视口可见的结束时间（毫秒）
    pub time_end_ms: f32,
    /// 当前视口可见的起始键位
    pub key_start: f32,
    /// 当前视口可见的结束键位
    pub key_end: f32,
    /// 整曲时长（毫秒）
    pub duration_ms: f32,
    /// 总键数（128 或 256）
    pub total_keys: f32,
}
