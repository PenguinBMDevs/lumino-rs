//! 高精度贴图 wgpu 渲染器
//!
//! 管理多个整合组贴图的 GPU 纹理，按视口可见性上传/淘汰，
//! 每帧绘制可见贴图。每张贴图覆盖一个 area 矩形（framebuffer 像素）。

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::config::HiResConfig;
use crate::types::TileCoord;

/// 着色器源码
const SHADER_SOURCE: &str = include_str!("shaders/hires_tile.wgsl");

/// 每张贴图的 uniform（32 字节，满足 16 字节对齐）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct HiResUniform {
    /// area 矩形在 framebuffer 中的 X（左上角）
    pub area_x: f32,
    /// area 矩形在 framebuffer 中的 Y（左上角）
    pub area_y: f32,
    /// area 矩形宽度
    pub area_w: f32,
    /// area 矩形高度
    pub area_h: f32,
    /// canvas 总宽度（像素）
    pub canvas_w: f32,
    /// canvas 总高度（像素）
    pub canvas_h: f32,
    _pad0: f32,
    _pad1: f32,
}

impl HiResUniform {
    pub fn new(
        area_x: f32,
        area_y: f32,
        area_w: f32,
        area_h: f32,
        canvas_w: f32,
        canvas_h: f32,
    ) -> Self {
        Self {
            area_x,
            area_y,
            area_w,
            area_h,
            canvas_w,
            canvas_h,
            _pad0: 0.0,
            _pad1: 0.0,
        }
    }
}

/// 单张贴图的 GPU 资源
///
/// `texture` 和 `view` 虽不直接读取，但必须保活以持有 GPU 资源所有权，
/// drop 时自动释放显存。
#[allow(dead_code)]
struct TileGpuResource {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    byte_size: usize,
}

/// 高精度贴图渲染器
pub struct HiResRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// 已上传的贴图纹理（TileCoord → GPU 资源）
    tiles: HashMap<TileCoord, TileGpuResource>,
    /// GPU 显存占用（字节）
    gpu_mem_used: usize,
    /// 配置（含显存上限等）
    config: HiResConfig,
}

impl HiResRenderer {
    /// 创建渲染器（pipeline + sampler + layout）
    pub fn new(device: &wgpu::Device, config: HiResConfig) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hires_tile_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hires_tile_bind_group_layout"),
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

        let pipeline = Self::create_pipeline(device, &bind_group_layout);

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            tiles: HashMap::new(),
            gpu_mem_used: 0,
            config,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hires_tile_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hires_tile_pipeline_layout"),
            bind_group_layouts: &[layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hires_tile_pipeline"),
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
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
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

    /// 上传一张贴图到 GPU
    pub fn upload_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        coord: TileCoord,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) {
        // 若已存在则先移除
        if self.tiles.contains_key(&coord) {
            self.remove_tile(&coord);
        }

        let byte_size = pixels.len();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("hires_tile_{coord:?}")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hires_tile_uniform"),
            size: std::mem::size_of::<HiResUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hires_tile_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.tiles.insert(
            coord,
            TileGpuResource {
                texture,
                view,
                bind_group,
                uniform_buffer,
                byte_size,
            },
        );
        self.gpu_mem_used += byte_size;
    }

    /// 移除一张贴图（释放显存）
    pub fn remove_tile(&mut self, coord: &TileCoord) {
        if let Some(gpu) = self.tiles.remove(coord) {
            self.gpu_mem_used = self.gpu_mem_used.saturating_sub(gpu.byte_size);
        }
    }

    /// 清空所有贴图
    pub fn clear(&mut self) {
        self.tiles.clear();
        self.gpu_mem_used = 0;
    }

    /// 准备可见贴图的 uniform（在 render_pass 开始前调用）
    pub fn prepare(&self, queue: &wgpu::Queue, visible: &[(TileCoord, HiResUniform)]) {
        for (coord, uniform) in visible {
            if let Some(gpu) = self.tiles.get(coord) {
                queue.write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(uniform));
            }
        }
    }

    /// 绘制可见贴图（在 render_pass 内调用）
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        visible_coords: &[TileCoord],
    ) {
        render_pass.set_pipeline(&self.pipeline);
        for coord in visible_coords {
            if let Some(gpu) = self.tiles.get(coord) {
                render_pass.set_bind_group(0, &gpu.bind_group, &[]);
                render_pass.draw(0..6, 0..1);
            }
        }
    }

    /// 检查贴图是否已上传
    pub fn has_tile(&self, coord: &TileCoord) -> bool {
        self.tiles.contains_key(coord)
    }

    /// 已上传贴图数量
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// GPU 显存占用（字节）
    pub fn gpu_mem_used(&self) -> usize {
        self.gpu_mem_used
    }

    /// GPU 显存上限（字节）
    pub fn gpu_mem_limit(&self) -> usize {
        (self.config.gpu_mem_limit_mb as usize) * 1024 * 1024
    }

    /// 显存是否超限
    pub fn is_over_limit(&self) -> bool {
        self.gpu_mem_used > self.gpu_mem_limit()
    }
}
