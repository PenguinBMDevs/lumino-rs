//! 音符渲染器类型定义

/// 音符实例数据 - 每个音符对应一个实例
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NoteInstance {
    /// 左上角位置 (x, y)
    pub position: [f32; 2],
    /// 尺寸 (width, height)
    pub size: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
}

impl NoteInstance {
    /// 创建新的音符实例
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            size: [width, height],
            color,
        }
    }
}

/// 视口 uniform 数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewportUniform {
    pub size: [f32; 2],
    pub _padding: [f32; 2],
}

impl ViewportUniform {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            size: [width, height],
            _padding: [0.0, 0.0],
        }
    }
}

/// 裁剪 uniform 数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CullUniform {
    pub instance_count: u32,
    pub _padding: [u32; 3],
}

/// 间接绘制参数
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndirectArgs {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
    pub _padding: [u32; 4],
}

/// 顶点属性布局（静态常量）
pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x2,
    },
    wgpu::VertexAttribute {
        offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x2,
    },
    wgpu::VertexAttribute {
        offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
];
