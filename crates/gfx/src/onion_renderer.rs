//! 洋葱皮 GPU 驱动渲染管线
//!
//! 完全消除 CPU 侧的音符遍历、矩形合并、顶点数据生成。
//! 使用 compute shader 进行可见性剔除，通过间接绘制实现实例化渲染。
//!
//! 数据流:
//!   1. 所有音轨音符平铺为 SoA 布局 → Storage Buffer 常驻 GPU
//!   2. 视口/轨道掩码变化时 → 调度 Compute Shader 剔除（dirty tracking 跳过无变化帧）
//!   3. 剔除结果 → Instance Index Buffer → draw_indexed_indirect
//!
//! 优化要点：
//!   - Bind group 仅在 buffer 重建时重建（非每帧）
//!   - Push constant 传 note_count / indices_capacity（替代 arrayLength）
//!   - GPU 端清零 indirect args（消除 CPU 冗余 write_buffer）
//!   - Dirty tracking 跳过静止帧的 compute dispatch
//!   - Buffer 按需扩容 + 空闲缩容

pub mod types;

pub use types::{
    CameraUniform, DrawIndirectArgs, OnionNote, OnionTrackColors, OnionTrackMask,
    OnionViewportUniform, TrackColor,
};

mod buffer;
mod cull;
mod init;
mod upload;

/// 洋葱皮 GPU 渲染器
pub struct OnionRenderer {
    // ─── GPU 资源 ──────────────────────────────────────
    /// 音符池 Storage Buffer（所有音轨的音符，SoA 布局）
    note_pool_buffer: wgpu::Buffer,
    /// 实例索引缓冲区（compute shader 输出）
    instance_indices_buffer: wgpu::Buffer,
    /// 间接绘制参数缓冲区
    indirect_buffer: wgpu::Buffer,
    /// 索引缓冲区（单位矩形，6 个顶点）
    index_buffer: wgpu::Buffer,
    /// 视口 uniform buffer
    viewport_buffer: wgpu::Buffer,
    /// 轨道掩码 uniform buffer
    track_mask_buffer: wgpu::Buffer,
    /// 轨道颜色 uniform buffer
    track_color_buffer: wgpu::Buffer,
    /// 相机 uniform buffer（复用 CameraUniform）
    camera_buffer: wgpu::Buffer,

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
    /// 实际音符数量
    note_count: usize,
    /// 实例索引缓冲区容量
    indices_capacity: usize,
    /// GPU 最大 storage buffer binding size
    max_storage_binding: u64,
    /// Bind group 是否需要重建（buffer 被重建时置 true）
    bind_groups_dirty: bool,
    /// 上一次 cull 的视口数据（用于 dirty tracking）
    last_viewport: Option<OnionViewportUniform>,
    /// 上一次 cull 的相机数据（用于 dirty tracking）
    last_camera: Option<CameraUniform>,
    /// 上一次 cull 的轨道掩码（用于 dirty tracking）
    last_track_mask: Option<OnionTrackMask>,
    /// 音符数据是否在上次 cull 后变化过
    notes_dirty: bool,
}

impl OnionRenderer {
    const INITIAL_NOTE_CAPACITY: usize = 8192;
    const INITIAL_INDICES_CAPACITY: usize = 65536;
    const INDICES_SHRINK_THRESHOLD: f64 = 0.25;
    const MAX_INDICES_CAPACITY: usize = 33_554_432;
    const WORKGROUP_SIZE: u32 = 256;

    const VERTEX_SHADER_SRC: &'static str =
        include_str!("onion_renderer/shaders/onion_render.wgsl");
    const COMPUTE_SHADER_SRC: &'static str = include_str!("onion_renderer/shaders/onion_cull.wgsl");
}

/// 将 OnionSkinColors（来自 ui crate）转换为 OnionTrackColors
/// 由 UI 层调用，传入数组 [r, g, b, a] 颜色值
pub fn convert_onion_colors(colors: &[(f32, f32, f32, f32)]) -> OnionTrackColors {
    let mut track_colors = OnionTrackColors::default();
    for (i, &(r, g, b, a)) in colors.iter().enumerate() {
        if i >= 64 {
            break;
        }
        track_colors.colors[i] = TrackColor::from_rgba(r, g, b, a);
    }
    track_colors
}
