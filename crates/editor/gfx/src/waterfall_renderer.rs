//! 瀑布流模式 GPU 渲染器
//!
//! 使用 compute shader 在 GPU 上直接渲染瀑布流帧，
//! 支持音符绘制、钢琴键盘（含活跃键高亮）、速度控制。
//!
//! # 生命周期
//!
//! 1. `new()` — 创建渲染器，编译 compute shader
//! 2. `render()` — 每帧调用：上传音符数据、dispatch compute shader、写入 storage texture
//! 3. `storage_texture()` — 获取输出纹理，供 export pipeline 读回

mod bind;
mod init;
mod render;

use crate::gpu_resource_tracker::{TrackedBuffer, TrackedTexture};

/// 单个瀑布流音符数据（与 waterfall.wgsl 中 WaterfallNote 匹配）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterfallNoteGpu {
    pub key: u32,
    pub start_tick: u32,
    pub end_tick: u32,
    pub color_packed: u32,
}

/// Uniform 参数（与 waterfall.wgsl 中 WaterfallUniform 匹配）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterfallUniformGpu {
    pub tick: u32,
    pub ppq: u32,
    pub key_count: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub kb_height: u32,
    pub speed: f32,
    pub _padding: u32,
}

/// 瀑布流 GPU 渲染器
pub struct WaterfallRenderer {
    compute_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,

    uniform_buffer: TrackedBuffer,
    note_buffer: Option<TrackedBuffer>,
    active_key_colors_buffer: Option<TrackedBuffer>,
    key_offsets_buffer: Option<TrackedBuffer>,

    output_texture: Option<TrackedTexture>,
    output_texture_view: Option<wgpu::TextureView>,

    note_capacity: usize,
    key_offsets_capacity: usize,
    current_width: u32,
    current_height: u32,
}
