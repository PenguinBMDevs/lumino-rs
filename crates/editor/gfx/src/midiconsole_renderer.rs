//! MidiConsole 复古终端 GPU 渲染器
//!
//! 将 MidiConsole 风格的字符网格（由 CPU 廉价构建）在 GPU 上栅格化：
//! - 字形图集（r8 覆盖率）在初始化时由 ab_glyph 烘焙一次并上传；
//! - 每帧仅上传网格单元（148×40 个 `CellGpu`，约 71KB）到只读存储缓冲；
//! - 全屏片元着色器完成字形采样、前/背景混合与 CRT 扫描线 + 移动高亮带；
//! - 输出 `Rgba8Unorm` 离屏纹理，可由导出管线读回 CPU 编码视频帧。
//!
//! 相比 CPU 路径（每帧对每字形做 ab_glyph 描边 + 全帧 CRT 逐像素），
//! GPU 路径将两项昂贵工作全部搬上 GPU，导出速度显著更快。

mod font;
mod gpu;
mod render;
mod types;

pub use types::pack_rgb;

/// 网格列数（与 CPU 风格一致）
pub const GRID_COLS: usize = 148;
/// 网格行数（与 CPU 风格一致）
pub const GRID_ROWS: usize = 40;

/// 字形图集列数（覆盖 96 个 ASCII 可打印 + 1 个半块 ▌ + 余量）
const ATLAS_COLS: u32 = 16;
/// 字形图集行数
const ATLAS_ROWS: u32 = 7;
/// 图集单槽像素宽（输出 cell 的 2 倍，保证清晰）
const ATLAS_CELL_W: u32 = 20;
/// 图集单槽像素高
const ATLAS_CELL_H: u32 = 40;
/// CRT 移动高亮带速度（像素/帧）
const BAND_SPEED: f32 = 6.0;

/// 单网格单元在 GPU 侧的数据布局（16 字节，storage 数组无填充歧义）
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CellGpu {
    /// 字符码点（空格 32 表示空单元）
    pub ch: u32,
    /// 前景色，打包为 0xRRGGBB
    pub fg: u32,
    /// 背景色，打包为 0xRRGGBB
    pub bg: u32,
    /// 填充对齐
    pub _pad: u32,
}

/// 片元着色器 uniform（64 字节，16 字节对齐）
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    grid_cols: u32,
    grid_rows: u32,
    cell_w: f32,
    cell_h: f32,
    atlas_cols: u32,
    atlas_rows: u32,
    atlas_cw: f32,
    atlas_ch: f32,
    frame_w: f32,
    frame_h: f32,
    band_center: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
}

/// MidiConsole GPU 终端渲染器
pub struct MidiconsoleRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    cells_buffer: wgpu::Buffer,
    output_texture: wgpu::Texture,
    output_texture_view: wgpu::TextureView,
    cell_w: f32,
    cell_h: f32,
    frame_w: u32,
    frame_h: u32,
}

/// GPU 渲染上下文（封装 wgpu 设备/队列与渲染器），供导出路径在多次帧之间复用。
///
/// 所有 wgpu 类型均封闭在 `lumino_gfx` 内部，调用方（如导出 handler）无需直接依赖 wgpu。
pub struct MidiconsoleGpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: MidiconsoleRenderer,
}

#[cfg(test)]
mod tests;
