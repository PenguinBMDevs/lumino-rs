//! 钢琴键盘 wgpu 离屏渲染器
//!
//! 将键盘绘制到一张离屏 `Rgba8Unorm` 纹理，读回 RGBA 字节后交由 iced 以
//! `image::Handle` 显示。渲染器管线（着色器 + 单位四边形顶点缓冲）只创建一次，
//! 按帧仅在参数（宽高 / 键数 / 主题）变化时重绘，读回后产出 `Handle`。
//!
//! 参考：编辑器主渲染器的 waterfall 风格钢琴键盘配色（经 `ThemeExt` 取色）。

use iced_wgpu::wgpu;
use wgpu::util::DeviceExt;

use tracing::warn;

use super::key_layout::{self, KeyRect};

/// 键盘渲染配色（0..1 线性）
pub struct KeyboardColors {
    /// 键盘背景（缝隙/留白）
    pub bg: [f32; 4],
    /// 白键填充色
    pub white: [f32; 4],
    /// 黑键填充色
    pub black: [f32; 4],
}

impl KeyboardColors {
    /// 纯黑白配色（白键/背景纯白，黑键纯黑），无边框/缝隙
    pub fn pure() -> Self {
        Self {
            bg: [1.0, 1.0, 1.0, 1.0],
            white: [1.0, 1.0, 1.0, 1.0],
            black: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// 键盘高度相对宽度的比例（面板宽度变化时高度联动）
///
/// 与视频导出渲染的瀑布流钢琴键盘保持一致：导出默认 1920×1080，
/// 键盘高 = 帧高 × 12% = 129.6px，键盘宽 = 帧宽 = 1920px，
/// → 高宽比 = 0.12 × (1080/1920) = 0.0675。
pub(crate) const KEY_HEIGHT_RATIO: f32 = 0.0675;
/// 键盘最小高度（像素）
pub(crate) const MIN_KEY_HEIGHT: f32 = 36.0;
/// 键盘最大高度（像素）
pub(crate) const MAX_KEY_HEIGHT: f32 = 140.0;
/// 面板内容内边距（用于计算键盘实际绘制宽度）
pub(crate) const PANEL_PADDING: f32 = 8.0;

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs(
    @location(0) pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
) -> VsOut {
    var o: VsOut;
    // rect.xy = 矩形左下角（clip 空间），rect.zw = 尺寸
    o.pos = vec4<f32>(rect.xy + pos * rect.zw, 0.0, 1.0);
    o.color = color;
    return o;
}

@fragment
fn fs(@location(0) color: vec4<f32>) -> @location(0) vec4<f32> {
    return color;
}
"#;

/// 单个实例数据：clip 空间矩形（xy=min, zw=size）+ 填充色
#[repr(C)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
}

/// 钢琴键盘离屏渲染器（持有管线与单位四边形，跨帧复用）
pub struct KeyboardRenderer {
    pipeline: wgpu::RenderPipeline,
    quad_buffer: wgpu::Buffer,
}

impl KeyboardRenderer {
    /// 创建渲染器（管线 + 单位四边形顶点缓冲）
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("piano_waterfall_keyboard"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("piano_waterfall_keyboard"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("piano_waterfall_keyboard"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[
                    // 单位四边形（顶点缓冲，6 个顶点两个三角形）
                    wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    },
                    // 实例缓冲（每个琴键一个实例）
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 0,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 2,
                            },
                        ],
                    },
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        // 单位正方形 [0,1]^2 → 两个三角形（6 顶点）
        let quad: [f32; 12] = [
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ];
        let quad_bytes = f32_slice_to_bytes(&quad);
        let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("piano_waterfall_keyboard_quad"),
            contents: &quad_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            quad_buffer,
        }
    }

    /// 渲染键盘到离屏纹理并读回 RGBA 字节（行优先，Rgba8）
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        keys: &[KeyRect],
        colors: &KeyboardColors,
    ) -> Vec<u8> {
        let width = width.max(1);
        let height = height.max(1);

        // wgpu 纹理→缓冲拷贝要求每行字节数为 256 对齐：宽度向上取整到 64 的倍数
        let padded_width = (width + 63) & !63;
        let bytes_per_row = padded_width * 4;

        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("piano_waterfall_keyboard_tex"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());

        let instances = build_instances(width, height, keys, colors);
        let instance_bytes = instances_to_bytes(&instances);
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("piano_waterfall_keyboard_instances"),
            contents: &instance_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_keyboard_staging"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bg = colors.bg;
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("piano_waterfall_keyboard_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: bg[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.pipeline);
            rp.set_vertex_buffer(0, self.quad_buffer.slice(..));
            rp.set_vertex_buffer(1, instance_buffer.slice(..));
            let count = instances.len() as u32;
            if count > 0 {
                rp.draw(0..6, 0..count);
            }
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        // 同步读回（面板键盘数据量极小，阻塞等待可接受）
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res.map_err(|e| e.to_string()));
        });
        if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
            warn!("钢琴键盘纹理映射轮询失败: {e}");
        }
        rx.recv()
            .expect("读回通道断开")
            .expect("键盘纹理映射失败");

        let mapped = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        let row_bytes = bytes_per_row as usize;
        let active = width as usize * 4;
        for y in 0..height as usize {
            let start = y * row_bytes;
            rgba.extend_from_slice(&mapped[start..start + active]);
        }
        drop(mapped);
        staging.unmap();

        rgba
    }

    /// 便捷方法：按布局渲染（纯黑白）并返回可直接用于 `Handle::from_rgba` 的字节
    pub fn render_keyboard(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        key_count: u32,
    ) -> Vec<u8> {
        let keys = key_layout::build_layout(width as f32, height as f32, key_count);
        let colors = KeyboardColors::pure();
        // 白键在前、黑键在后，保证黑键覆盖在白键之上
        let mut ordered = keys;
        ordered.sort_by_key(|k| k.is_black);
        self.render(device, queue, width, height, &ordered, &colors)
    }
}

/// 像素矩形 → clip 空间实例数据，并排好绘制顺序（白键 → 黑键）
fn build_instances(
    width: u32,
    height: u32,
    keys: &[KeyRect],
    colors: &KeyboardColors,
) -> Vec<Instance> {
    let w = width as f32;
    let h = height as f32;
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        let color = if key.is_black {
            colors.black
        } else {
            colors.white
        };
        // 像素矩形 → clip 空间（y 向下 → 需翻转）
        let rx = (key.x / w) * 2.0 - 1.0;
        let ry = 1.0 - ((key.y + key.h) / h) * 2.0;
        let rw = (key.w / w) * 2.0;
        let rh = (key.h / h) * 2.0;
        out.push(Instance {
            rect: [rx, ry, rw, rh],
            color: [color[0], color[1], color[2], color[3]],
        });
    }
    out
}

/// `&[f32]` → `&[u8]`（小端，避免引入 bytemuck 依赖）
fn f32_slice_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// `&[Instance]` → `Vec<u8>`（按内存布局逐字段小端写入）
fn instances_to_bytes(instances: &[Instance]) -> Vec<u8> {
    let mut out = Vec::with_capacity(instances.len() * 32);
    for inst in instances {
        for f in inst.rect.iter().chain(inst.color.iter()) {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    out
}
