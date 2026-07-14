use std::collections::{HashMap, VecDeque};

use crate::config::HiResConfig;
use crate::types::TileCoord;

use super::texture::TileGpuResource;
use super::uniform::HiResUniform;

/// 着色器源码
const SHADER_SOURCE: &str = include_str!("../shaders/hires_tile.wgsl");

/// 高精度贴图渲染器
pub struct HiResRenderer {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) bind_group_layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
    /// 已上传的贴图纹理（TileCoord → GPU 资源）
    pub(super) tiles: HashMap<TileCoord, TileGpuResource>,
    /// 编辑后的临时脏区域贴图覆层（叠加在正常贴图之上）
    pub(super) dirty_overlays: HashMap<TileCoord, TileGpuResource>,
    /// GPU 显存占用（字节）
    pub(super) gpu_mem_used: usize,
    /// 配置（含显存上限等）
    pub(super) config: HiResConfig,
    /// 贴图上传顺序（用于 FIFO 逐出，最早上传的先被逐出）
    tile_order: VecDeque<TileCoord>,
}

impl HiResRenderer {
    /// 创建渲染器（pipeline + sampler + layout）
    pub fn new(
        device: &wgpu::Device,
        config: HiResConfig,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
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

        let pipeline = Self::create_pipeline(device, &bind_group_layout, texture_format);

        Self {
            pipeline,
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
                    format: color_format,
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
        // 注意：不移除 dirty_overlays！这是有意为之——后台流式接收（GenerateHiResOnionSkin
        // 或 RegenerateHiResTrack）与 ShowHiResDirtyOverlay 在同一帧循环中先后执行，
        // 若在此处清除覆层，新上传的覆盖层会在同一帧被后台贴图流误清除，导致用户
        // 永远看不到临时脏区域覆层。脏覆层在 upload_dirty_overlay 替换同坐标覆层时
        // 自然清理，或在 dispose_hires_onion_skin 全量释放。

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
        self.tile_order.push_back(coord);
        self.evict_if_over_limit();
    }

    /// 移除一张贴图（释放显存）
    pub fn remove_tile(&mut self, coord: &TileCoord) {
        if let Some(gpu) = self.tiles.remove(coord) {
            self.gpu_mem_used = self.gpu_mem_used.saturating_sub(gpu.byte_size);
        }
        self.tile_order.retain(|c| c != coord);
    }

    /// 清空所有贴图
    pub fn clear(&mut self) {
        self.tiles.clear();
        self.dirty_overlays.clear();
        self.gpu_mem_used = 0;
        self.tile_order.clear();
    }

    /// 清空指定音轨组的临时脏区域覆层
    pub fn clear_dirty_overlays(&mut self, track_group: u32) {
        self.dirty_overlays
            .retain(|coord, _| coord.track_group != track_group);
    }

    /// 上传一张临时脏区域贴图覆层
    pub fn upload_dirty_overlay(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        coord: TileCoord,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) {
        if let Some(gpu) = self.dirty_overlays.remove(&coord) {
            self.gpu_mem_used = self.gpu_mem_used.saturating_sub(gpu.byte_size);
        }

        let byte_size = pixels.len();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("hires_dirty_overlay_{coord:?}")),
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
            label: Some("hires_dirty_overlay_uniform"),
            size: std::mem::size_of::<HiResUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hires_dirty_overlay_bind_group"),
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

        self.dirty_overlays.insert(
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

    /// 准备可见贴图的 uniform（在 render_pass 开始前调用）
    pub fn prepare(&self, queue: &wgpu::Queue, visible: &[(TileCoord, HiResUniform)]) {
        for (coord, uniform) in visible {
            if let Some(gpu) = self.tiles.get(coord) {
                queue.write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(uniform));
            }
        }
    }

    /// 检查贴图是否已上传
    pub fn has_tile(&self, coord: &TileCoord) -> bool {
        self.tiles.contains_key(coord)
    }

    /// 检查临时脏区域覆层是否已上传
    pub fn has_dirty_overlay(&self, coord: &TileCoord) -> bool {
        self.dirty_overlays.contains_key(coord)
    }

    /// 检查贴图或临时脏区域覆层是否已上传
    pub fn has_tile_or_dirty_overlay(&self, coord: &TileCoord) -> bool {
        self.tiles.contains_key(coord) || self.dirty_overlays.contains_key(coord)
    }

    /// 已上传贴图数量
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// 临时脏区域覆层数量
    pub fn dirty_overlay_count(&self) -> usize {
        self.dirty_overlays.len()
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

    /// 超出显存上限时，按 FIFO 顺序逐出最早上传的贴图
    ///
    /// 调度器按时间顺序生成贴图，逐出旧时间段的贴图不会影响当前可见区域。
    /// 若用户滚动到已逐出的时间段，调度器会重新生成对应贴图。
    fn evict_if_over_limit(&mut self) {
        while self.is_over_limit() {
            match self.tile_order.pop_front() {
                None => break,
                Some(coord) => {
                    if self.tiles.contains_key(&coord) {
                        self.remove_tile(&coord);
                    }
                }
            }
        }
    }

    /// 更新渲染目标格式（surface 格式变化时重建 pipeline）
    pub fn update_render_format(
        &mut self,
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
    ) {
        self.pipeline = Self::create_pipeline(device, &self.bind_group_layout, color_format);
    }
}
