//! CPU 洋葱皮叠加层的管线、uniform 与 bind group layout。
//!
//! 深度兼容说明：UI 渲染通道带 `DEPTH_FORMAT` 的 depth attachment，管线因此携带
//! **格式匹配但完全只读**的 depth-stencil 状态（不写深度、`Always` 通过）。
//! 若用 `None`，wgpu 会以「管线无 depth 状态 / pass 有 depth attachment」拒绝绘制。
//! 视频导出路径没有 depth attachment，本步不接导出，也不为此新增第二条管线。

use crate::constants::rendering::DEPTH_FORMAT;
use crate::gpu_resource_tracker::TrackedBuffer;
use crate::pipeline::RenderPipelineBuilder;
use crate::shader::create_shader_module;

use super::frame::CpuOnionFrame;

/// 叠加层纹理格式。
///
/// CPU 光栅输出的是 **sRGB 编码字节**，因此纹理必须声明为 sRGB：采样时硬件做
/// sRGB→linear 解码，片元直接返回该 linear 值，写入 sRGB 主目标时硬件再
/// linear→sRGB 编码。alpha = 255 的像素由此逐字节往返一致。
pub(super) const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// 片元/顶点着色器源码。
const SHADER_SOURCE: &str = include_str!("../shaders/cpu_onion.wgsl");

/// 顶点与片元共享的 uniform（物理视口 + 物理矩形）。
///
/// 两个 `vec4<f32>`（32 字节）满足 uniform 地址空间的 16 字节对齐/尺寸要求。
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct CpuOnionUniform {
    /// xy = 物理视口尺寸（像素），zw = 保留
    pub(super) viewport: [f32; 4],
    /// xy = 物理像素左上角，zw = 物理像素尺寸
    pub(super) rect: [f32; 4],
}

impl CpuOnionUniform {
    /// 由物理视口与帧矩形构造。
    pub(super) fn new(viewport: (u32, u32), frame: &CpuOnionFrame) -> Self {
        Self {
            viewport: [viewport.0 as f32, viewport.1 as f32, 0.0, 0.0],
            rect: [
                frame.x as f32,
                frame.y as f32,
                frame.width as f32,
                frame.height as f32,
            ],
        }
    }

    /// 全零初值（创建 uniform 缓冲时的占位内容）。
    pub(super) fn zeroed() -> Self {
        Self {
            viewport: [0.0; 4],
            rect: [0.0; 4],
        }
    }
}

/// 创建 uniform 缓冲区（32 字节，`UNIFORM | COPY_DST`）。
pub(super) fn create_uniform_buffer(device: &wgpu::Device) -> TrackedBuffer {
    TrackedBuffer::new(
        device,
        &wgpu::BufferDescriptor {
            label: Some("cpu_onion_uniform"),
            size: std::mem::size_of::<CpuOnionUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
    )
}

/// 创建 bind group layout：uniform（顶点）+ 纹理（片元）+ 采样器（片元）。
pub(super) fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cpu_onion_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// 创建叠加层渲染管线（三角形列表，6 顶点 = 1 个四边形）。
pub(super) fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = create_shader_module(device, "cpu_onion_shader", SHADER_SOURCE);
    RenderPipelineBuilder::new(device, "cpu_onion_pipeline", &shader)
        .bind_group(layout)
        .alpha_blended_target(format)
        .depth_stencil(Some(depth_stencil_state()))
        .build()
}

/// 与带 depth attachment 的 UI RenderPass 兼容的只读深度状态。
///
/// - `depth_write_enabled: false` — 叠加层不参与深度裁决，不污染后续绘制；
/// - `depth_compare: Always` — 显式恒通过（不依赖清屏值 / 比较函数的浮点语义）；
/// - `bias: DepthBiasState::default()` — depth_bias 全 0。
pub(super) fn depth_stencil_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::Always,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}
