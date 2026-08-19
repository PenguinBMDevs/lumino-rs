use std::collections::{HashMap, VecDeque};

use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::types::WaterfallTileCoord;

use super::texture::TileGpuResource;

mod tiles;

/// 着色器源码
const SHADER_SOURCE: &str = include_str!("../shaders/texture_waterfall.wgsl");

/// 贴图瀑布流渲染器
pub struct TextureWaterfallRenderer {
    pub(super) pipeline: wgpu::RenderPipeline,
    /// 视频导出等无 depth attachment 的 RenderPass 使用的管线
    pub(super) pipeline_no_depth: wgpu::RenderPipeline,
    pub(super) bind_group_layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
    /// 已上传的贴图纹理（WaterfallTileCoord → GPU 资源）
    pub(super) tiles: HashMap<WaterfallTileCoord, TileGpuResource>,
    /// 编辑后的临时脏区域贴图覆层（叠加在正常贴图之上）
    pub(super) dirty_overlays: HashMap<WaterfallTileCoord, TileGpuResource>,
    /// GPU 显存占用（字节）
    pub(super) gpu_mem_used: usize,
    /// 配置（含显存上限等）
    ///
    /// 用户硬约束：不得限制 GPU 内存使用——gpu_mem_limit() 已改为返回 usize::MAX，
    /// config 不再用于显存限制决策，保留字段用于其他配置项（tile_width_px 等）的潜在读取。
    #[allow(dead_code)]
    pub(super) config: TextureWaterfallConfig,
    /// 贴图上传顺序（用于 FIFO 逐出，最早上传的先被逐出）
    tile_order: VecDeque<WaterfallTileCoord>,
}

impl TextureWaterfallRenderer {
    /// 创建渲染器（pipeline + sampler + layout）
    pub fn new(
        device: &wgpu::Device,
        config: TextureWaterfallConfig,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture_waterfall_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_waterfall_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
        });

        let pipeline = Self::create_pipeline(device, &bind_group_layout, texture_format, true);
        let pipeline_no_depth =
            Self::create_pipeline(device, &bind_group_layout, texture_format, false);

        Self {
            pipeline,
            pipeline_no_depth,
            bind_group_layout,
            sampler,
            tiles: HashMap::new(),
            dirty_overlays: HashMap::new(),
            gpu_mem_used: 0,
            config,
            tile_order: VecDeque::new(),
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("texture_waterfall_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("texture_waterfall_pipeline_layout"),
            bind_group_layouts: &[layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("texture_waterfall_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: needs_depth.then_some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// 更新渲染目标格式（surface 格式变化时重建 pipeline）
    pub fn update_render_format(
        &mut self,
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
    ) {
        self.pipeline = Self::create_pipeline(device, &self.bind_group_layout, color_format, true);
        self.pipeline_no_depth =
            Self::create_pipeline(device, &self.bind_group_layout, color_format, false);
    }

    /// 根据 RenderPass 是否携带 depth attachment 选择对应的管线。
    #[inline]
    pub(super) fn pipeline_for(&self, has_depth: bool) -> &wgpu::RenderPipeline {
        if has_depth {
            &self.pipeline
        } else {
            &self.pipeline_no_depth
        }
    }
}
