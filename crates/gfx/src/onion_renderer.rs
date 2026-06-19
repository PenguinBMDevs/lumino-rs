//! 洋葱皮 GPU 渲染管线
//!
//! 数据流:
//!   1. upload_notes → 音符颜色注入 → GPU storage buffer（全量上传）
//!   2. prepare_cull → Compute Shader 剔除（视口 + 当前音轨）→ instance_indices buffer
//!   3. prepare_viewport → 更新视口 uniform
//!   4. draw → indirect draw（TriangleStrip + 4 顶点/实例）
//!
//! 方向：钢琴卷帘方向（X=time, Y=pitch）
//!
//! 参考 Wasabi:
//! - gui/window/scene/note_list_system/mod.rs (NoteRenderer::draw)
//! - shaders/notes/notes.geom → WGSL vertex shader equivalent

pub mod types;

pub use types::{DrawIndirectArgs, OnionNote, OnionViewportUniform};

mod buffer;
mod init;
mod upload;

/// 洋葱皮渲染器 — GPU compute cull + indirect draw
pub struct OnionRenderer {
    // ─── GPU 资源 ──────────────────────────────────────
    /// 音符池 Storage Buffer（全量音符，compute shader 读取）
    note_pool_buffer: wgpu::Buffer,
    /// 实例索引缓冲区（compute shader 输出 → vertex shader 输入）
    instance_indices_buffer: wgpu::Buffer,
    /// 间接绘制参数缓冲区（compute shader 写入 instance_count）
    indirect_buffer: wgpu::Buffer,
    /// 视口 uniform buffer
    viewport_buffer: wgpu::Buffer,

    // ─── Pipeline ──────────────────────────────────────
    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    compute_bind_group: wgpu::BindGroup,
    render_bind_group: wgpu::BindGroup,
    compute_bind_group_layout: wgpu::BindGroupLayout,
    render_bind_group_layout: wgpu::BindGroupLayout,

    // ─── 状态 ──────────────────────────────────────────
    /// 音符池容量（OnionNote 数量）
    note_pool_capacity: usize,
    /// 实例索引缓冲区容量
    indices_capacity: usize,
    /// 全量音符数量（compute shader dispatch 用）
    total_note_count: u32,
    /// GPU max storage buffer binding size
    max_storage_binding: u64,
    /// CPU 侧临时音符缓冲（跨帧复用）
    cpu_note_pool: Vec<OnionNote>,
    /// 上一次上传的 list 版本号
    last_list_version: u64,
    /// 上一次上传的颜色哈希值
    last_color_hash: u64,
    /// 上一次的视口数据（dirty tracking）
    last_viewport: Option<OnionViewportUniform>,
}

impl OnionRenderer {
    const INITIAL_NOTE_CAPACITY: usize = 8192;
    const INITIAL_INDICES_CAPACITY: usize = 65536;
    const MAX_NOTE_POOL_CAPACITY: usize = 33_554_432;
    const MAX_INDICES_CAPACITY: usize = 33_554_432;
    const WORKGROUP_SIZE: u32 = 256;

    const VERTEX_SHADER_SRC: &'static str = include_str!("./shaders/onion_render.wgsl");
    const COMPUTE_SHADER_SRC: &'static str = include_str!("./shaders/onion_cull.wgsl");

    pub fn note_count(&self) -> u32 {
        self.total_note_count
    }

    pub fn indices_capacity(&self) -> usize {
        self.indices_capacity
    }
}
