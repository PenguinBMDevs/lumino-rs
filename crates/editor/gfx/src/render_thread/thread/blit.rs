use super::WgpuRenderThread;

impl WgpuRenderThread {
    /// 将离屏渲染结果复制到 Surface 纹理
    ///
    /// 在主线程调用，将渲染线程生成的离屏纹理复制到当前 Surface。
    /// 等尺寸时走高速 `copy_texture_to_texture`；尺寸失配（如 macOS 最大化/拖拽动画期
    /// Surface 已新尺寸而离屏仍为旧尺寸）则走采样 `blit` 拉伸填充，避免全黑或黑边
    /// （对标 yinhe `uv_max` 采样拉伸，无额外黑边，零额外全黑帧）。
    pub fn copy_offscreen_to_surface(
        &self,
        frame: &wgpu::SurfaceTexture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let texture_ref = self.latest_texture.try_lock().ok().and_then(|g| g.clone());

        let Some(texture) = texture_ref else {
            return;
        };

        let tex_w = texture.inner().width();
        let tex_h = texture.inner().height();
        let frame_w = frame.texture.width();
        let frame_h = frame.texture.height();

        // 等尺寸快路径：直接内存拷贝，零采样开销
        if tex_w == frame_w && tex_h == frame_h {
            puffin::profile_scope!("copy_offscreen_texture");
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy_offscreen_texture_encoder"),
            });
            encoder.copy_texture_to_texture(
                texture.inner().as_image_copy(),
                frame.texture.as_image_copy(),
                wgpu::Extent3d {
                    width: tex_w,
                    height: tex_h,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(std::iter::once(encoder.finish()));
            return;
        }

        // 尺寸失配：采样 blit 拉伸（macOS 动画期/拖拽 resize 高频触发，~1 帧过渡）
        tracing::debug!(
            "copy_offscreen size mismatch tex={}x{} frame={}x{} — blit stretch",
            tex_w,
            tex_h,
            frame_w,
            frame_h
        );
        Self::blit_texture_to_surface(
            device,
            queue,
            texture.inner(),
            &frame.texture,
            frame.texture.format(),
        );
    }

    /// 采样 blit：将 `src` 纹理线性拉伸绘制到 `dst`（SurfaceTexture）全屏
    ///
    /// 用于最大化/拖拽动画期 `src` 与 `dst` 尺寸不一致时，避免 `copy_texture_to_texture`
    /// 的等尺寸限制导致的黑屏/黑边。离屏需 `TEXTURE_BINDING`（已在 textures.rs 配置）。
    fn blit_texture_to_surface(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::Texture,
        dst: &wgpu::Texture,
        dst_format: wgpu::TextureFormat,
    ) {
        use std::sync::{Mutex, OnceLock};

        const SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) idx: u32) -> VOut {
    var pos = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var uv = array<vec2<f32>, 3>(vec2<f32>(0.0, 1.0), vec2<f32>(2.0, 1.0), vec2<f32>(0.0, -1.0));
    var out: VOut; out.pos = vec4<f32>(pos[idx], 0.0, 1.0); out.uv = uv[idx]; return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;

        struct BlitCache {
            pipeline: wgpu::RenderPipeline,
            bind_group_layout: wgpu::BindGroupLayout,
            sampler: wgpu::Sampler,
            format: wgpu::TextureFormat,
        }
        static CACHE: OnceLock<Mutex<Option<BlitCache>>> = OnceLock::new();
        let cache_mutex = CACHE.get_or_init(|| Mutex::new(None));

        // 尝试复用缓存管线（format 需一致，否则重建）
        let needs_rebuild = {
            let guard = cache_mutex.lock().ok();
            guard
                .as_ref()
                .and_then(|opt| opt.as_ref().map(|c| c.format != dst_format))
                .unwrap_or(true)
        };

        if needs_rebuild {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("blit_sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            });
            let bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("blit_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("blit_shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("blit_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("blit_pipeline"),
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
                        format: dst_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            if let Ok(mut guard) = cache_mutex.lock() {
                *guard = Some(BlitCache {
                    pipeline,
                    bind_group_layout,
                    sampler,
                    format: dst_format,
                });
            }
        }

        let cache_guard = match cache_mutex.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let Some(cache) = cache_guard.as_ref() else {
            return;
        };

        // 为当前 src 创建视图与绑定组
        let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit_bind_group"),
            layout: &cache.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&cache.sampler),
                },
            ],
        });

        let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("blit_encoder"),
        });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&cache.pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}
