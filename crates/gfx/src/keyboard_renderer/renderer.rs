//! 键盘渲染器实现

/// 键盘渲染器
pub struct KeyboardRenderer {
    /// 渲染管线
    pub(super) pipeline: wgpu::RenderPipeline,
    /// 实例缓冲区
    pub(super) instance_buffer: wgpu::Buffer,
    /// 视口 uniform 缓冲区
    pub(super) viewport_buffer: wgpu::Buffer,
    /// Bind group
    pub(super) bind_group: wgpu::BindGroup,
    /// 当前缓冲区容量（实例数量）
    pub(super) capacity: usize,
    /// 白键颜色
    pub(super) white_key_color: [f32; 4],
    /// 黑键颜色
    pub(super) black_key_color: [f32; 4],
    /// 选中键颜色
    pub(super) selected_key_color: [f32; 4],
}

impl KeyboardRenderer {
    /// 初始缓冲区容量（128键钢琴）
    pub(super) const INITIAL_CAPACITY: usize = 128;
    /// 缓冲区扩容因子
    pub(super) const GROWTH_FACTOR: usize = 2;
    /// 顶点着色器代码
    pub(super) const VERTEX_SHADER: &'static str = include_str!("../shaders/keyboard.wgsl");
}

mod generator;
mod init;
mod instance;
mod render;
#[cfg(test)]
mod tests;
