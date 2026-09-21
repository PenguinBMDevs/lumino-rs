//! Miditrail 3D 视频导出渲染器（wgpu 渲染管线实现）
//!
//! 该渲染器以 3D 透视方式渲染 MIDI 键盘与音符轨迹，结果写入离屏纹理，
//! 再由导出管线读回 CPU 并编码为视频帧。

mod aura;
mod buffers;
mod cull;
mod diag;
mod driven;
mod instances;
mod key_press;
mod math;
mod pipeline;
mod quantize;
mod render;
mod render_pass;
mod textures;
mod types;

/// 重导出颜色打包工具（供视频导出 waterfall/miditrail 模式使用）
pub use instances::pack_color;
pub use types::{
    MiditrailAuraInstanceGpu, MiditrailCameraGpu, MiditrailDrivenParamsGpu, MiditrailInstanceGpu,
    MiditrailNoteGpu, MiditrailUniformGpu, MiditrailViewMode, miditrail_viewport_span,
};

use crate::NoteInstance;
use aura::{create_aura_buffers, create_aura_sampler, generate_aura_ring_data};
use instances::{
    ActiveKeys, NoteBuildScratch, build_aura_instances, build_key_instances, build_note_instances,
    compute_active_and_aura_for_compact, compute_active_keys, emit_aura_instances,
    update_key_positions,
};
use math::build_camera_uniform;
use pipeline::{
    create_aura_render_pipeline, create_bind_group_layout, create_buffers,
    create_driven_group_layout, create_note_driven_pipeline, create_note_render_pipeline,
    create_quad_index_buffer, create_render_pipeline, create_top_note_render_pipeline,
    create_top_render_pipeline,
};
use quantize::quantize_notes_for_top;

const KEY_PRESS_SPEED_DOWN: f32 = 15.0;
const KEY_PRESS_SPEED_UP: f32 = 10.0;
const AURA_TEXTURE_SIZE: u32 = 128;

/// 3D 场景深度（tick 到 Z 坐标的映射比例）。
pub const MIDITRAIL_SCENE_DEPTH: f32 = 7.5;
/// Z 方向显示距离默认值（与场景深度相同）。
pub const MIDITRAIL_DEFAULT_Z_FAR_DISTANCE: f32 = 7.5;
/// Z 方向显示距离最大值（也是音符收集范围的最大倍数）。
pub const MIDITRAIL_MAX_Z_FAR_DISTANCE: f32 = 15.0;

/// Top 键盘去按压用的全零系数（俯视只留颜色反馈；切回 Normal 时内部
/// `key_press_factors` 仍在更新，按压动画无缝衔接）。
static ZERO_PRESS_FACTORS: [f32; 128] = [0.0; 128];

/// 3D MIDITrail 渲染器
///
/// 使用实例化立方体渲染键盘与音符，结果写入 `Rgba8Unorm` 离屏纹理。
/// Normal 与 Top 视图共用实例缓冲/纹理/深度（零第二份显存），
/// 区别仅在于相机、音符精度（Top 逐音量化对齐、永不合并）、键盘反馈
/// （Top 无按压位移、只变色）与着色器（Top flat）。
pub struct MiditrailRenderer {
    render_pipeline: wgpu::RenderPipeline,
    note_pipeline: wgpu::RenderPipeline,
    top_render_pipeline: wgpu::RenderPipeline,
    top_note_pipeline: wgpu::RenderPipeline,
    /// GPU-Driven 音符管线（Normal 终局路径：compact 直传＋vertex 推导＋深度排序）。
    driven_note_pipeline: wgpu::RenderPipeline,
    /// Driven 参数组布局（group1：位姿参数＋键位表 uniform）。
    driven_group_layout: wgpu::BindGroupLayout,
    /// Driven 参数绑定（params 缓冲创建后初始化一次，缓冲句柄稳定）。
    driven_bind_group: Option<wgpu::BindGroup>,
    /// Driven 参数上传缓冲（约 1KB，每帧重写）。
    driven_params_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    /// 紧凑音符上传缓冲（`NoteInstance` 原字节直传，16B/音符）。
    compact_buffer: Option<crate::gpu_resource_tracker::TrackedBuffer>,
    compact_capacity: usize,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,

    uniform_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    vertex_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    index_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    /// 音符平面索引缓冲（`QUAD_INDICES`，仅音符 draw 绑定；琴键/Aura 不动）。
    quad_index_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    /// 音符平面模式（`3D音符` 开关关闭 = 平面）：盒子→单面，12→2 三角形/音符。
    /// 只切换音符 draw 的索引缓冲与绘制段；实例/顺序/变换/颜色/其他物体零改动。
    /// 构造默认 false（盒子，现状零行为变化）；导出 handler 每帧按参数显式设置。
    pub flat_notes: bool,
    instance_buffer: Option<crate::gpu_resource_tracker::TrackedBuffer>,

    output_texture: Option<crate::gpu_resource_tracker::TrackedTexture>,
    output_texture_view: Option<wgpu::TextureView>,
    depth_texture: Option<crate::gpu_resource_tracker::TrackedTexture>,
    depth_texture_view: Option<wgpu::TextureView>,

    instance_capacity: usize,
    current_width: u32,
    current_height: u32,

    key_positions: Vec<f32>,
    key_widths: Vec<f32>,
    last_key_count: u32,
    key_press_factors: [f32; 128],
    /// 实例构建暂存集（跨帧复用，避免每帧大堆分配；渲染循环单线程独占）。
    ///
    /// 高密度导出（70 万可见音符）下实例缓冲约 33MB × 2、排序索引约 11MB × 2，
    /// 每帧新建是毫秒级开销；复用后仅首次分配。
    scratch_build: NoteBuildScratch,
    scratch_notes: Vec<MiditrailInstanceGpu>,
    scratch_keys: Vec<MiditrailInstanceGpu>,
    scratch_auras: Vec<MiditrailAuraInstanceGpu>,
    /// `NoteInstance` → `MiditrailNoteGpu` 换算暂存（跨帧复用；36 万可见时约
    /// 11.7MB，每帧新建是毫秒级分配器开销）。仅视频导出 `render_from_instances` 用。
    scratch_derived: Vec<MiditrailNoteGpu>,
    /// 导出全量常驻缓冲（首帧一次上传，`seed_resident`；无 CPU 镜像）。
    resident_buffer: Option<crate::gpu_resource_tracker::TrackedBuffer>,
    resident_capacity: usize,
    resident_count: usize,
    /// 常驻窗口提取器（桶一次构建，常驻复用；见 `cull.rs`）。
    resident_cull: crate::ResidentCull,
    /// cull compact 回读暂存（V×16B，按需扩容）与 CPU 切片（跨帧复用）。
    cull_staging: Option<crate::gpu_resource_tracker::TrackedBuffer>,
    cull_cpu: Vec<crate::NoteInstance>,

    // Aura 相关资源
    aura_pipeline: wgpu::RenderPipeline,
    aura_vertex_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    aura_index_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    aura_instance_buffer: Option<crate::gpu_resource_tracker::TrackedBuffer>,
    aura_instance_capacity: usize,
    aura_sampler: wgpu::Sampler,
    aura_texture: Option<crate::gpu_resource_tracker::TrackedTexture>,
    aura_texture_view: Option<wgpu::TextureView>,
    aura_image_data: Vec<u8>,
    aura_resources_ready: bool,
}

impl MiditrailRenderer {
    const SHADER: &'static str = include_str!("shaders/miditrail_3d.wgsl");
    const TOP_SHADER: &'static str = include_str!("shaders/miditrail_top.wgsl");
    const AURA_SHADER: &'static str = include_str!("shaders/miditrail_aura.wgsl");
    const DRIVEN_SHADER: &'static str = include_str!("shaders/miditrail_note_driven.wgsl");
    // 单位立方体，每面 4 个顶点，含法线（位置 + 法线 = 6 个 f32）
    const CUBE_VERTICES: [f32; 144] = [
        // 顶面 y=1, normal (0,1,0)
        0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0,
        0.0, 1.0, 1.0, 0.0, 1.0, 0.0, // 底面 y=0, normal (0,-1,0)
        0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0,
        0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, // 正面 z=1, normal (0,0,1)
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0,
        0.0, 1.0, 1.0, 0.0, 0.0, 1.0, // 背面 z=0, normal (0,0,-1)
        0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 1.0, 0.0, 0.0, 0.0,
        -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0, // 左面 x=0, normal (-1,0,0)
        0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, 0.0,
        0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, // 右面 x=1, normal (1,0,0)
        1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0,
        1.0, 1.0, 0.0, 1.0, 0.0, 0.0,
    ];
    const CUBE_INDICES: [u16; 36] = [
        // 顶面
        0, 1, 2, 0, 2, 3, // 底面
        4, 6, 5, 4, 7, 6, // 正面
        8, 9, 10, 8, 10, 11, // 背面
        12, 14, 13, 12, 15, 14, // 左面
        16, 17, 18, 16, 18, 19, // 右面
        20, 21, 22, 20, 22, 23,
    ];
    /// 音符平面索引（盒子→平面的唯一改动点）：仅顶面 6（y=1 的 X-Z 面）。
    /// 音符实例语义（`instances.rs`）：scale=[宽, 高 NOTE_HEIGHT, Z 长]，
    /// Y 即高度轴；平面 = 只压 Y，X（音高宽）/Z（时值长）原样保留。
    /// 索引逐字复用 `CUBE_INDICES[0..6]` 绕序——同顶点/同插值/同 shader，
    /// 顶面片元与盒子顶面逐位一致；Normal/Top 双视图统一画本面
    /// （Normal 下盒子可见像素本就几乎全来自顶面：高仅 0.007，
    /// 正面只是 z=1 处一条细边；误取正面=把 Z 长压没了）。
    /// 琴键/Aura 继续走立方体缓冲，本缓冲仅音符 draw 绑定。
    const QUAD_INDICES: [u16; 6] = [
        // 顶面 y=1（X-Z 面：X 宽保留，Z 长保留，Y 压平）
        0, 1, 2, 0, 2, 3,
    ];
    /// 平面索引在 `QUAD_INDICES` 中的位置（Normal/Top 双视图共用）。
    const QUAD_RANGE: std::ops::Range<u32> = 0..6;

    const INITIAL_INSTANCE_CAPACITY: usize = 4096;

    /// 创建 Miditrail 渲染器。
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = crate::shader::create_shader_module(device, "miditrail_shader", Self::SHADER);
        let top_shader =
            crate::shader::create_shader_module(device, "miditrail_top_shader", Self::TOP_SHADER);
        let aura_shader =
            crate::shader::create_shader_module(device, "miditrail_aura_shader", Self::AURA_SHADER);
        let driven_shader = crate::shader::create_shader_module(
            device,
            "miditrail_driven_shader",
            Self::DRIVEN_SHADER,
        );
        let bind_group_layout = create_bind_group_layout(device);
        let driven_group_layout = create_driven_group_layout(device);
        let render_pipeline = create_render_pipeline(device, &bind_group_layout, &shader);
        let note_pipeline = create_note_render_pipeline(device, &bind_group_layout, &shader);
        let top_render_pipeline =
            create_top_render_pipeline(device, &bind_group_layout, &top_shader);
        let top_note_pipeline =
            create_top_note_render_pipeline(device, &bind_group_layout, &top_shader);
        let driven_note_pipeline = create_note_driven_pipeline(
            device,
            &bind_group_layout,
            &driven_group_layout,
            &driven_shader,
        );
        let driven_params_buffer = crate::gpu_resource_tracker::TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("miditrail_driven_params_buffer"),
                size: std::mem::size_of::<MiditrailDrivenParamsGpu>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        let aura_pipeline = create_aura_render_pipeline(device, &bind_group_layout, &aura_shader);
        let (uniform_buffer, vertex_buffer, index_buffer) =
            create_buffers(device, &Self::CUBE_VERTICES, &Self::CUBE_INDICES);
        let quad_index_buffer = create_quad_index_buffer(device, &Self::QUAD_INDICES);
        let (aura_vertex_buffer, aura_index_buffer) = create_aura_buffers(device);
        let aura_sampler = create_aura_sampler(device);
        let aura_image_data = generate_aura_ring_data(AURA_TEXTURE_SIZE);

        Self {
            render_pipeline,
            note_pipeline,
            top_render_pipeline,
            top_note_pipeline,
            driven_note_pipeline,
            driven_group_layout,
            driven_bind_group: None,
            driven_params_buffer,
            compact_buffer: None,
            compact_capacity: 0,
            bind_group_layout,
            bind_group: None,
            uniform_buffer,
            vertex_buffer,
            index_buffer,
            quad_index_buffer,
            flat_notes: false,
            instance_buffer: None,
            output_texture: None,
            output_texture_view: None,
            depth_texture: None,
            depth_texture_view: None,
            instance_capacity: 0,
            current_width: 0,
            current_height: 0,
            key_positions: Vec::new(),
            key_widths: Vec::new(),
            last_key_count: 0,
            key_press_factors: [0.0; 128],
            scratch_build: NoteBuildScratch::default(),
            scratch_notes: Vec::new(),
            scratch_keys: Vec::new(),
            scratch_auras: Vec::new(),
            scratch_derived: Vec::new(),
            resident_buffer: None,
            resident_capacity: 0,
            resident_count: 0,
            resident_cull: crate::ResidentCull::new(),
            cull_staging: None,
            cull_cpu: Vec::new(),
            aura_pipeline,
            aura_vertex_buffer,
            aura_index_buffer,
            aura_instance_buffer: None,
            aura_instance_capacity: 0,
            aura_sampler,
            aura_texture: None,
            aura_texture_view: None,
            aura_image_data,
            aura_resources_ready: false,
        }
    }

    /// 获取输出纹理引用。
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref().map(|t| t.inner())
    }
}

#[cfg(test)]
mod tests;
