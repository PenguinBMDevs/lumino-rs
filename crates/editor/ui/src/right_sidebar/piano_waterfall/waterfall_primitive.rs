//! 钢琴瀑布流「直接 GPU 合成」图元
//!
//! 与钢琴卷帘洋葱皮同一思路：离屏纹理（`KeyboardRenderer` 渲染产物）由 iced `shader` 图元
//! 在 iced 自身的渲染通道内**直接采样合成**（GPU→GPU），不经过 CPU 读回、不进 `image::Handle`、
//! 不进 iced 图集。这正是主卷帘不闪烁而旧 `image::Handle` 路径闪烁的根因差异——本图元消除该差异。

use std::sync::{Arc, Mutex};

use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};
use iced_wgpu::wgpu;

/// 全屏三角形（两三角覆盖 clip 空间 [-1,1]），在图元已受限的 viewport/scissor 内填充面板区域。
const WATERFALL_PRIMITIVE_SHADER: &str = r#"
struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var out: VSOut;
    let p = positions[vi];
    out.pos = vec4<f32>(p, 0.0, 1.0);
    // 纹理行 0 为瀑布流顶部；clip y=+1 对应顶部 → 翻转到 uv.y
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, 1.0 - (p.y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, uv);
}
"#;

/// 瀑布流图元管线：一个「纹理采样全屏 quad」的极简渲染管线。
///
/// 绑定组（纹理视图随每帧瀑布流纹理变化）缓存于管线内部；`Primitive` 仅持有纹理视图，
/// 从而满足 `Primitive: Debug + Send + Sync` 约束（绑定组不在 Primitive 内）。
pub struct WaterfallPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// 当前绑定的离屏纹理视图（用于判定是否需要重建绑定组）
    cached_view: Option<Arc<wgpu::TextureView>>,
    /// 离屏纹理 → 采样器 的绑定组（随视图变化重建）
    bind_group: Mutex<Option<Arc<wgpu::BindGroup>>>,
}

impl Pipeline for WaterfallPipeline {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("piano_waterfall_primitive_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
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

        let shader =
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("piano_waterfall_primitive"),
                source: wgpu::ShaderSource::Wgsl(WATERFALL_PRIMITIVE_SHADER.into()),
            });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("piano_waterfall_primitive"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("piano_waterfall_primitive_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            })),
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
                    // 直alpha混合：纹理透明背景（a=0）透出面板；音符/键盘不透明（a=1）直接覆盖
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("piano_waterfall_primitive_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            cached_view: None,
            bind_group: Mutex::new(None),
        }
    }

    fn trim(&mut self) {}
}

/// 瀑布流图元：持有离屏纹理视图，在 iced 渲染通道内直接采样合成。
///
/// 仅持 `Arc<TextureView>`（满足 `Debug + Send + Sync`）；绑定组缓存于管线内部。
#[derive(Debug)]
pub struct WaterfallPrimitive {
    /// 离屏纹理视图（`Arc` 克隆，纹理重建时旧视图仍可被在途图元安全引用）
    view: Arc<wgpu::TextureView>,
}

impl WaterfallPrimitive {
    /// 用离屏纹理视图构造图元
    pub fn new(view: Arc<wgpu::TextureView>) -> Self {
        Self { view }
    }
}

impl Primitive for WaterfallPrimitive {
    type Pipeline = WaterfallPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _bounds: &iced_wgpu::graphics::core::Rectangle,
        _viewport: &Viewport,
    ) {
        // 视图变化（或首次）时重建绑定组，避免每帧无条件重建
        if pipeline.cached_view.as_ref() != Some(&self.view) {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("piano_waterfall_primitive_bg"),
                layout: &pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(self.view.as_ref()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                ],
            });
            *pipeline.bind_group.lock().expect("waterfall bind_group lock") =
                Some(Arc::new(bg));
            pipeline.cached_view = Some(self.view.clone());
        }
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let guard = pipeline.bind_group.lock().expect("waterfall bind_group lock");
        match guard.as_ref() {
            Some(bg) => {
                render_pass.set_pipeline(&pipeline.pipeline);
                render_pass.set_bind_group(0, bg.as_ref(), &[]);
                render_pass.draw(0..6, 0..1);
                true
            }
            None => false,
        }
    }
}
