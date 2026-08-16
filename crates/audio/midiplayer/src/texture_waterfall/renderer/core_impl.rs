use std::collections::{HashMap, VecDeque};

use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::types::WaterfallTileCoord;

use super::texture::TileGpuResource;
use super::uniform::TextureWaterfallUniform;

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

    /// 上传一张贴图到 GPU
    pub fn upload_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        coord: WaterfallTileCoord,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) {
        // 若已存在则先移除
        if self.tiles.contains_key(&coord) {
            self.remove_tile(&coord);
        }
        // 注意：不移除 dirty_overlays！这是有意为之——后台流式接收（GenerateTextureWaterfall
        // 或 RegenerateTextureWaterfallTrack）与 ShowTextureWaterfallDirtyOverlay 在同一帧循环中先后执行，
        // 若在此处清除覆层，新上传的覆盖层会在同一帧被后台贴图流误清除，导致用户
        // 永远看不到临时脏区域覆层。脏覆层在 upload_dirty_overlay 替换同坐标覆层时
        // 自然清理，或在 dispose_TextureWaterfall_onion_skin 全量释放。

        let byte_size = pixels.len();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("texture_waterfall_{coord:?}")),
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
            label: Some("texture_waterfall_uniform"),
            size: std::mem::size_of::<TextureWaterfallUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_waterfall_bind_group"),
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
        // 用户硬约束：不得限制 GPU 内存使用——删除 evict_if_over_limit 淘汰逻辑，
        // 所有上传的贴图常驻 GPU 显存，避免滚动到已淘汰时段时贴图瀑布流音符消失。
    }

    /// 移除一张贴图（释放显存）
    pub fn remove_tile(&mut self, coord: &WaterfallTileCoord) {
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
        coord: WaterfallTileCoord,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) {
        if let Some(gpu) = self.dirty_overlays.remove(&coord) {
            self.gpu_mem_used = self.gpu_mem_used.saturating_sub(gpu.byte_size);
        }

        let byte_size = pixels.len();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("TextureWaterfall_dirty_overlay_{coord:?}")),
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
            label: Some("TextureWaterfall_dirty_overlay_uniform"),
            size: std::mem::size_of::<TextureWaterfallUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("TextureWaterfall_dirty_overlay_bind_group"),
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
    pub fn prepare(
        &self,
        queue: &wgpu::Queue,
        visible: &[(WaterfallTileCoord, TextureWaterfallUniform)],
    ) {
        for (coord, uniform) in visible {
            if let Some(gpu) = self.tiles.get(coord) {
                queue.write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(uniform));
            }
        }
    }

    /// 检查贴图是否已上传
    pub fn has_tile(&self, coord: &WaterfallTileCoord) -> bool {
        self.tiles.contains_key(coord)
    }

    /// 检查临时脏区域覆层是否已上传
    pub fn has_dirty_overlay(&self, coord: &WaterfallTileCoord) -> bool {
        self.dirty_overlays.contains_key(coord)
    }

    /// 检查贴图或临时脏区域覆层是否已上传
    pub fn has_tile_or_dirty_overlay(&self, coord: &WaterfallTileCoord) -> bool {
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
    ///
    /// 用户硬约束：不得限制 GPU 内存使用。返回 usize::MAX 表示无限制。
    pub fn gpu_mem_limit(&self) -> usize {
        usize::MAX
    }

    /// 显存是否超限
    ///
    /// 用户硬约束：不得限制 GPU 内存使用。此函数始终返回 false，
    /// 保留方法以兼容外部查询接口（如统计面板显示）。
    pub fn is_over_limit(&self) -> bool {
        false
    }

    /// 显存淘汰逻辑（已禁用）
    ///
    /// 用户硬约束：不得限制 GPU 内存使用，不得淘汰已上传贴图。
    /// 保留为空实现以维持 API 兼容（外部可能有调用）。
    #[allow(dead_code)]
    fn evict_if_over_limit(&mut self) {
        // no-op：贴图常驻 GPU 显存
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
