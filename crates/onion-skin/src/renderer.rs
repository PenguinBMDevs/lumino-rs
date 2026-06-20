//! OnionSkinRenderer — 构造、渲染管线、uniform 更新、绘制

use crate::types::{KeyMode, ViewportParams};
use crate::uniform::OnionSkinUniform;

/// 洋葱皮概览贴图渲染器
pub struct OnionSkinRenderer {
    /// wgpu 纹理
    pub(crate) texture: Option<wgpu::Texture>,
    /// wgpu 纹理视图
    pub(crate) texture_view: Option<wgpu::TextureView>,
    /// 渲染管线
    pub(crate) pipeline: wgpu::RenderPipeline,
    /// Uniform 缓冲区
    pub(crate) uniform_buffer: wgpu::Buffer,
    /// Bind group
    pub(crate) bind_group: wgpu::BindGroup,
    /// Bind group layout
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    /// 采样器
    pub(crate) sampler: wgpu::Sampler,
    /// 贴图是否已就绪（upload 完成）
    pub(crate) ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// 当前 key 模式
    pub(crate) key_mode: KeyMode,
    /// 当前曲目时长（毫秒）
    pub(crate) duration_ms: u32,
    /// 后台生成线程的 JoinHandle
    pub(crate) generate_thread: Option<std::thread::JoinHandle<()>>,
    /// 从后台线程接收进度
    pub(crate) progress_rx: Option<std::sync::mpsc::Receiver<crate::types::GenerateProgress>>,
    /// 从后台线程接收结果
    pub(crate) result_rx: Option<std::sync::mpsc::Receiver<crate::types::GenerateResult>>,
    /// 通知后台线程取消的标志
    pub(crate) cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl OnionSkinRenderer {
    /// WGSL 着色器代码
    const SHADER_SOURCE: &'static str = include_str!("shaders/onion_skin.wgsl");

    /// 创建新的洋葱皮渲染器
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, key_mode: KeyMode) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("onion_skin_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("onion_skin_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_skin_uniform_buffer"),
            size: std::mem::size_of::<OnionSkinUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (placeholder_texture, placeholder_view) =
            Self::create_placeholder_texture(device, queue);

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onion_skin_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&placeholder_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline =
            Self::create_pipeline(device, &bind_group_layout, wgpu::TextureFormat::Rgba8Unorm);

        Self {
            texture: Some(placeholder_texture),
            texture_view: Some(placeholder_view),
            pipeline,
            uniform_buffer,
            bind_group,
            bind_group_layout,
            sampler,
            ready: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            key_mode,
            duration_ms: 0,
            generate_thread: None,
            progress_rx: None,
            result_rx: None,
            cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 创建 1x1 占位纹理
    fn create_placeholder_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("onion_skin_placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("onion_skin_placeholder_view"),
            ..Default::default()
        });

        let zero_pixel: [u8; 4] = [0, 0, 0, 0];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &zero_pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        (texture, view)
    }

    /// 创建渲染管线
    fn create_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("onion_skin_pipeline_layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_skin_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SOURCE.into()),
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("onion_skin_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
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

    /// 更新渲染目标格式（在 surface 创建后调用）
    pub fn update_render_format(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.pipeline = Self::create_pipeline(device, &self.bind_group_layout, format);
    }

    /// 更新 uniform 数据（每帧调用）
    pub fn update_uniform(&self, queue: &wgpu::Queue, params: ViewportParams) {
        let uniform = OnionSkinUniform {
            area_x: params.area_x,
            area_y: params.area_y,
            area_w: params.area_w,
            area_h: params.area_h,
            time_start_ms: params.time_start_ms,
            time_end_ms: params.time_end_ms,
            key_start: params.key_start,
            key_end: params.key_end,
            duration_ms: self.duration_ms as f32,
            total_keys: self.key_mode.total_keys(),
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// 绘制洋葱皮贴图到 render pass
    pub fn render<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        if !self.ready.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}

impl Drop for OnionSkinRenderer {
    fn drop(&mut self) {
        self.dispose();
    }
}
