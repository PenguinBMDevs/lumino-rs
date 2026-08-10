//! 音符渲染器类型定义

/// 预览音符的 border_width 哨兵值。
/// 当 `border_width == PREVIEW_BORDER_SENTINEL` 时，FS 走预览分支（70% alpha，不画边框）。
pub const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFF_FFFF;

/// 音符逻辑实例数据 — 16 bytes，严格对齐 wasabi `NoteVertex`
///
/// 字段布局完全复刻 wasabi（参考 `wasabi/src/gui/window/scene/note_list_system/notes_render_pass.rs:41-50`）：
///   - `start_length`: `[f32; 2]` — `[start, length]`，单位 tick（保留 lumino 编辑器语义）
///   - `key_color`: `u32` — 低 8 位 = MIDI key，高 24 位 = RGB（无 alpha，与 wasabi 一致）
///   - `border_width`: `u32` — 低 16 位 = 边框像素宽度，高 16 位 = 轨道深度编码
///     （洋葱皮 track_idx+1，VS 据此输出稳定深度解决重叠音符闪烁；
///     主音轨高 16 位为 0 → z=0.0 最前；`PREVIEW_BORDER_SENTINEL` 表示预览音符）
///
/// 与 wasabi 的唯一差异：`start`/`length` 单位保留 tick（lumino 是 DAW 编辑器，tick 是底层语义），
/// 其余 GPU 侧数据存放逻辑完全一致。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NoteInstance {
    /// `[start_tick, length_tick]`，与 wasabi `start_length` 字段名/字节对齐
    pub start_length: [f32; 2],
    /// 低 8 位 = key，高 24 位 = RGB（与 wasabi `key_color` 编码一致）
    pub key_color: u32,
    /// 边框像素宽度；`PREVIEW_BORDER_SENTINEL` 表示预览音符
    pub border_width: u32,
}

/// 将 `(key, color)` 编码为 wasabi 风格的 `key_color` u32
/// 低 8 位 = key，高 24 位 = RGB（每通道 8 位，无 alpha）
#[must_use]
pub fn pack_key_color(key: u8, color: [f32; 4]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u32;
    let rgb = (r << 16) | (g << 8) | b;
    (key as u32) | (rgb << 8)
}

/// 将 `key_color` u32 解码为 `(key, rgba)`
/// alpha 不存于 `key_color`，恒为 1.0（与 wasabi 一致）
#[must_use]
pub fn unpack_key_color(packed: u32) -> (u8, [f32; 4]) {
    let key = (packed & 0xFF) as u8;
    let rgb = packed >> 8;
    let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
    let b = (rgb & 0xFF) as f32 / 255.0;
    (key, [r, g, b, 1.0])
}

impl NoteInstance {
    /// 创建新的音符逻辑实例（普通音符）
    /// `key` 为 u8（0-255，支持 256 键，与 wasabi `NoteVertex::new` 一致）
    /// `border_width` 低 16 位由 `calculate_border_width` 算出，主音轨所有音符共享同一值；
    /// 高 16 位为轨道深度编码（洋葱皮由 `onion_border_width` 编码，主音轨为 0）
    #[must_use]
    pub fn new(tick: f32, key: u8, length: f32, color: [f32; 4], border_width: u32) -> Self {
        Self {
            start_length: [tick, length],
            key_color: pack_key_color(key, color),
            border_width,
        }
    }

    /// 创建预览音符（`border_width = PREVIEW_BORDER_SENTINEL`，FS 走预览分支）
    /// `key` 为 u8（0-255，与 wasabi 一致）
    #[must_use]
    pub fn new_preview(tick: f32, key: u8, length: f32, color: [f32; 4]) -> Self {
        Self {
            start_length: [tick, length],
            key_color: pack_key_color(key, color),
            border_width: PREVIEW_BORDER_SENTINEL,
        }
    }
}

/// 计算音符边框像素宽度（复刻 wasabi `utils::calculate_border_width`）
///
/// 参考：`wasabi/src/utils.rs:13-15`
/// ```ignore
/// pub fn calculate_border_width(width_pixels: f32, keys_len: f32) -> f32 {
///     ((width_pixels / keys_len) / 12.0).clamp(1.0, 5.0).round() * 2.0
/// }
/// ```
///
/// 主音轨所有音符共享同一 `zoom.y`，因此 CPU 端只算一次填所有音符（D2=C 决策）。
#[must_use]
pub fn calculate_border_width(width_pixels: f32, keys_len: f32) -> u32 {
    if keys_len <= 0.0 {
        return 0;
    }
    let raw = ((width_pixels / keys_len) / 12.0).clamp(1.0, 5.0).round() * 2.0;
    raw as u32
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
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
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

/// 顶点属性布局 — 16 bytes NoteInstance（与 wasabi NoteVertex 字段对齐）
/// 3 个属性：start_length(vec2) / key_color(u32) / border_width(u32)
pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x2, // start_length: [start, length]
    },
    wgpu::VertexAttribute {
        // start_length(8) = 8
        offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
        shader_location: 1,
        format: wgpu::VertexFormat::Uint32, // key_color
    },
    wgpu::VertexAttribute {
        // start_length(8) + key_color(4) = 12
        offset: (std::mem::size_of::<[f32; 2]>() + std::mem::size_of::<u32>())
            as wgpu::BufferAddress,
        shader_location: 2,
        format: wgpu::VertexFormat::Uint32, // border_width
    },
];
