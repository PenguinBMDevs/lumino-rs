//! 音符渲染器类型定义

/// 音符逻辑实例数据 — 16 bytes 紧凑布局
///
/// 优化（参考 wasabi）：
///   1. `size_y` 固定为 1.0（GPU 通过 zoom_y 展开），移除 4 bytes
///   2. `color` 从 [f32;4] 压缩为 u32 RGBA，移除 12 bytes
///
/// 总计 32 → 16 bytes，GPU 数据量减半，上传带宽减半
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NoteInstance {
    /// 逻辑位置: [tick, key]
    pub position: [f32; 2],
    /// 逻辑长度（height 固定为 1.0，在 GPU 中通过 zoom_y 展开）
    pub size_x: f32,
    /// 颜色 RGBA 打包 (0xRRGGBBAA)
    pub color_packed: u32,
}

/// 将 [f32; 4] 颜色打包为 u32 (0xRRGGBBAA)
#[must_use]
pub fn pack_color(color: [f32; 4]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (color[3].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// 将 u32 打包颜色解包为 [f32; 4]
#[must_use]
pub fn unpack_color(packed: u32) -> [f32; 4] {
    let r = ((packed >> 24) & 0xFF) as f32 / 255.0;
    let g = ((packed >> 16) & 0xFF) as f32 / 255.0;
    let b = ((packed >> 8) & 0xFF) as f32 / 255.0;
    let a = (packed & 0xFF) as f32 / 255.0;
    [r, g, b, a]
}

impl NoteInstance {
    /// 创建新的音符逻辑实例
    #[must_use]
    pub fn new(tick: f32, key: f32, length: f32, color: [f32; 4]) -> Self {
        Self {
            position: [tick, key],
            size_x: length,
            color_packed: pack_color(color),
        }
    }
}

/// 洋葱皮背景瓦片引用 — 16 bytes，与 NoteInstance 对齐
///
/// 每个实例代表一个音轨在某个 tick×key 格子中的可见区域，
/// GPU 通过 `track_index` 从 color LUT 中查找颜色进行渲染。
///
/// 设计说明：
/// - `track_index` 解耦颜色：颜色/透明度变化只需更新 LUT，无需重建 buffer
/// - `bytemuck::Pod + Zeroable` 直接 upload 到 GPU storage buffer
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionBgTileRef {
    /// 逻辑位置: [tick_start, key]
    pub position: [f32; 2],
    /// 逻辑尺寸: [tick_span, key_span] — 瓦片覆盖的范围
    pub size: [f32; 2],
    /// 音轨索引（GPU 从 color LUT 中查找颜色）
    pub track_index: u32,
    /// 保留字段（未来扩展用）
    pub _padding: u32,
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

/// 顶点属性布局（静态常量）— 16 bytes 紧凑 NoteInstance
pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x2, // position
    },
    wgpu::VertexAttribute {
        offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32, // size_x
    },
    wgpu::VertexAttribute {
        // position(8) + size_x(4) = 12
        offset: (std::mem::size_of::<[f32; 2]>() + std::mem::size_of::<f32>())
            as wgpu::BufferAddress,
        shader_location: 2,
        format: wgpu::VertexFormat::Uint32, // color_packed
    },
];
