//! 音符渲染器类型定义

/// 音符逻辑实例数据 - GPU 侧通过 CameraUniform 计算最终屏幕位置
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NoteInstance {
    /// 逻辑位置: [tick, key]
    pub position: [f32; 2],
    /// 逻辑尺寸: [length, 1.0]（height 固定为1个key，在GPU中通过 zoom_y 展开）
    pub size: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
}

impl NoteInstance {
    /// 创建新的音符逻辑实例
    #[must_use]
    pub const fn new(tick: f32, key: f32, length: f32, color: [f32; 4]) -> Self {
        Self {
            position: [tick, key],
            size: [length, 1.0],
            color,
        }
    }
}

/// 摄像机/视口 uniform 数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub scroll: [f32; 2],
    pub zoom: [f32; 2],
    pub viewport_size: [f32; 2],
    pub canvas_offset: [f32; 2],
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub max_key_index: f32,
    pub _padding: f32,
}

pub struct CameraParams {
    pub scroll: [f32; 2],
    pub zoom: [f32; 2],
    pub viewport: [f32; 2],
    pub offset: [f32; 2],
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub max_key_index: f32,
}

impl CameraUniform {
    #[must_use]
    pub const fn new(params: CameraParams) -> Self {
        Self {
            scroll: params.scroll,
            zoom: params.zoom,
            viewport_size: params.viewport,
            canvas_offset: params.offset,
            keyboard_width: params.keyboard_width,
            ruler_height: params.ruler_height,
            max_key_index: params.max_key_index,
            _padding: 0.0,
        }
    }
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            scroll: [0.0, 0.0],
            zoom: [1.0, 1.0],
            viewport_size: [0.0, 0.0],
            canvas_offset: [0.0, 0.0],
            keyboard_width: 0.0,
            ruler_height: 0.0,
            max_key_index: 0.0,
            _padding: 0.0,
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

/// 合并的渲染 uniform 数据（Camera + Cull）
/// 用于单次上传，减少 CPU-GPU 往返
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RenderUniform {
    pub camera: CameraUniform,
    pub cull: CullUniform,
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

impl Default for DrawIndirectArgs {
    fn default() -> Self {
        Self {
            vertex_count: 4,
            instance_count: 0,
            first_vertex: 0,
            first_instance: 0,
            _padding: [0; 4],
        }
    }
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
