//! 钢琴瀑布流面板离屏渲染器（键盘底条 + 下落式音符）
//!
//! 将「下落式音符 + 底部钢琴键盘」绘制到一张离屏 `Rgba8Unorm` 纹理，读回 RGBA
//! 字节后交由 iced 以 `image::Handle` 显示。
//!
//! 关键约束（来自需求）：**音符数据禁止第二份拷贝**——直接 bind 渲染线程已持有的
//! 活体 GPU 音符实例缓冲（只读 `storage`），在面板 offscreen pass 中复用，不重新上传。
//! 音符外观/颜色逐位复刻主渲染器 `onion_note.wgsl`：调色板色 + 主音轨蓝覆盖。

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

/// 键盘底条着色器（单位四边形 + 实例矩形）
const KEYBOARD_SHADER: &str = r#"
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

/// 下落式音符着色器（复用渲染线程活体 GPU 实例缓冲，只读 storage）
///
/// 与 `onion_note.wgsl` 保持一致的取色逻辑：调色板色（unpack_key_color）+
/// 主音轨蓝覆盖（`border_width >> 16 == current_track`）。
/// 纵向映射：把卷帘的 x 轴旋转 90° 到面板 y 轴——底部键盘线 = 当前时间（min_tick），
/// 后续 tick 自顶部流入、向下落到键盘（瀑布流）。
const NOTE_SHADER: &str = r#"
struct NoteInstance {
    start_length: vec2<f32>,   // x = start_tick, y = length_tick
    key_color: u32,            // 低 8 位 = key，高 24 位 = RGB（无 alpha）
    border_width: u32,         // 高 16 位 = track_idx+1（主音轨判定）
};

struct NoteUniforms {
    resolution: vec2<f32>,     // 面板内容宽、全高
    zoom_x: f32,               // 每 tick 像素数（与卷帘 X 缩放一致）
    scroll_x: f32,             // 卷帘水平滚动（像素）
    current_track: u32,        // 主音轨编码 = current_track_idx + 1
    key_count: u32,
    keyboard_y: f32,           // 键盘顶边 y（瀑布流落点线）
    _pad: f32,
};

@group(0) @binding(0) var<storage, read> notes: array<NoteInstance>;
@group(0) @binding(1) var<uniform> u: NoteUniforms;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);

fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

fn is_black_key(k: i32) -> bool {
    let m = k % 12;
    return m == 1 || m == 3 || m == 6 || m == 8 || m == 10;
}

// 返回 [0, k) 范围内的白键数量（用于键位 x 定位）
fn whites_before(k: i32) -> i32 {
    let oct = k / 12;
    let rem = k % 12;
    var wb: i32 = 0;
    if (rem >= 1) { wb = wb + 1; }   // C#
    if (rem >= 3) { wb = wb + 1; }   // D#
    if (rem >= 5) { wb = wb + 1; }   // E
    if (rem >= 6) { wb = wb + 1; }   // F
    if (rem >= 8) { wb = wb + 1; }   // G#
    if (rem >= 10) { wb = wb + 1; }  // A#
    if (rem >= 11) { wb = wb + 1; }  // B
    return oct * 7 + wb;
}

@vertex
fn vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let cpos = corners[vid];

    let inst = notes[iid];
    let k = i32(inst.key_color & 0xFFu);
    let white_count = whites_before(i32(u.key_count));
    let white_w = u.resolution.x / f32(white_count);
    let black_w = white_w * 0.58;
    let wi = whites_before(k);

    var x: f32;
    var w: f32;
    if (is_black_key(k)) {
        x = f32(wi) * white_w - black_w * 0.5;
        w = black_w;
    } else {
        x = f32(wi) * white_w;
        w = white_w;
    }

    // y 轴：底部键盘线 = min_tick，后续 tick 向上（panel 顶部流入）
    let start = inst.start_length.x;
    let len = inst.start_length.y;
    let y_top = u.keyboard_y - ((start + len) * u.zoom_x - u.scroll_x);
    let y_bot = u.keyboard_y - (start * u.zoom_x - u.scroll_x);
    let px_left = x;
    let px_right = x + w;
    let corner_x = select(px_left, px_right, cpos.x > 0.5);
    let corner_y = select(y_top, y_bot, cpos.y > 0.5);

    // 视口裁剪：完全在面板外则折叠为退化三角形（不栅格化）
    var clip = vec2<f32>(2.0, 2.0);
    if (!(y_bot < 0.0 || y_top > u.resolution.y || px_right < 0.0 || px_left > u.resolution.x)) {
        clip = vec2<f32>(
            corner_x / u.resolution.x * 2.0 - 1.0,
            1.0 - corner_y / u.resolution.y * 2.0,
        );
    }

    var out: VsOut;
    out.pos = vec4<f32>(clip, 0.0, 1.0);

    let track_enc = inst.border_width >> 16u;
    var color = unpack_key_color(inst.key_color);
    if (track_enc == u.current_track) {
        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
    }
    out.color = color;
    return out;
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

/// 音符 uniform（std140：vec2 对齐 8，其余顺排，总 32 字节）
#[repr(C)]
struct NoteUniforms {
    resolution: [f32; 2],
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
    key_count: u32,
    keyboard_y: f32,
    _pad: f32,
}

/// 钢琴瀑布流离屏渲染器（持有管线与单位四边形，跨帧复用）
pub struct KeyboardRenderer {
    /// 键盘底条管线
    pipeline: wgpu::RenderPipeline,
    /// 单位四边形顶点缓冲
    quad_buffer: wgpu::Buffer,
    /// 下落式音符管线
    note_pipeline: wgpu::RenderPipeline,
    /// 音符 bind group layout（只读 storage 实例 + uniform）
    note_bind_group_layout: wgpu::BindGroupLayout,
    /// 音符 uniform 缓冲（每帧 write_buffer 更新）
    uniform_buffer: wgpu::Buffer,
}

impl KeyboardRenderer {
    /// 创建渲染器（键盘 + 音符两套管线 + uniform 缓冲）
    pub fn new(device: &wgpu::Device) -> Self {
        let keyboard_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("piano_waterfall_keyboard"),
            source: wgpu::ShaderSource::Wgsl(KEYBOARD_SHADER.into()),
        });
        let note_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("piano_waterfall_note"),
            source: wgpu::ShaderSource::Wgsl(NOTE_SHADER.into()),
        });

        let keyboard_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("piano_waterfall_keyboard"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let note_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("piano_waterfall_note_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let note_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("piano_waterfall_note"),
                bind_group_layouts: &[&note_bind_group_layout],
                push_constant_ranges: &[],
            });

        let keyboard_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("piano_waterfall_keyboard"),
                layout: Some(&keyboard_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &keyboard_shader,
                    entry_point: Some("vs"),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: 8,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 0,
                            }],
                        },
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
                    module: &keyboard_shader,
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

        let note_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("piano_waterfall_note"),
            layout: Some(&note_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &note_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &note_shader,
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_note_uniform"),
            size: std::mem::size_of::<NoteUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline: keyboard_pipeline,
            quad_buffer,
            note_pipeline,
            note_bind_group_layout,
            uniform_buffer,
        }
    }

    /// 渲染「下落式音符 + 底部键盘」到离屏纹理并读回 RGBA 字节（行优先，Rgba8）
    ///
    /// - `note_data`：渲染线程发布的活体 GPU 实例缓冲与实例数；`None` 时仅渲染键盘。
    /// - `zoom_x` / `scroll_x`：与钢琴卷帘 X 缩放/滚动一致，驱动音符落点与时间流。
    /// - `current_track`：主音轨编码（`current_track_idx + 1`），用于蓝色覆盖。
    #[allow(clippy::too_many_arguments)]
    pub fn render_scene(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        key_count: u32,
        note_data: Option<(wgpu::Buffer, u32)>,
        zoom_x: f32,
        scroll_x: f32,
        current_track: u32,
    ) -> Vec<u8> {
        let width = width.max(1);
        let height = height.max(1);

        // 键盘底条：高度按宽度比例联动，贴底
        let kb_h = (width as f32 * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
        let keyboard_y = height as f32 - kb_h;

        // wgpu 纹理→缓冲拷贝要求每行字节数为 256 对齐：宽度向上取整到 64 的倍数
        let padded_width = (width + 63) & !63;
        let bytes_per_row = padded_width * 4;

        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("piano_waterfall_tex"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());

        let colors = KeyboardColors::pure();
        let keys = key_layout::build_layout(width as f32, kb_h, key_count);
        let mut ordered = keys;
        ordered.sort_by_key(|k| k.is_black); // 白键在前、黑键在后，黑键覆盖白键
        let instances = build_instances(width, kb_h, keyboard_y, &ordered, &colors);
        let instance_bytes = instances_to_bytes(&instances);
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("piano_waterfall_keyboard_instances"),
            contents: &instance_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });

        // 音符 uniform
        let uni = build_uniforms(
            width as f32,
            height as f32,
            zoom_x,
            scroll_x,
            current_track,
            key_count,
            keyboard_y,
        );
        queue.write_buffer(&self.uniform_buffer, 0, &uni);

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_staging"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bg = colors.bg;
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("piano_waterfall_pass"),
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

            // 1) 下落式音符（复用渲染线程活体 GPU 实例缓冲，禁止第二份拷贝）
            if let Some((buf, count)) = &note_data
                && *count > 0
            {
                let bg_note = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("piano_waterfall_note_bg"),
                    layout: &self.note_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(
                                buf.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(
                                self.uniform_buffer.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
                rp.set_pipeline(&self.note_pipeline);
                rp.set_bind_group(0, &bg_note, &[]);
                rp.draw(0..6, 0..*count);
            }

            // 2) 底部钢琴键盘（覆盖在落点线处，确保键位清晰）
            rp.set_pipeline(&self.pipeline);
            rp.set_vertex_buffer(0, self.quad_buffer.slice(..));
            rp.set_vertex_buffer(1, instance_buffer.slice(..));
            let kcount = instances.len() as u32;
            if kcount > 0 {
                rp.draw(0..6, 0..kcount);
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

        // 同步读回（面板数据量极小，阻塞等待可接受）
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res.map_err(|e| e.to_string()));
        });
        if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
            warn!("钢琴瀑布流纹理映射轮询失败: {e}");
        }
        rx.recv()
            .expect("读回通道断开")
            .expect("瀑布流纹理映射失败");

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
}

/// 像素矩形 → clip 空间实例数据（白键 → 黑键排序），并整体下移 `y_offset` 贴底
fn build_instances(
    width: u32,
    height: f32,
    y_offset: f32,
    keys: &[KeyRect],
    colors: &KeyboardColors,
) -> Vec<Instance> {
    let w = width as f32;
    let h = height;
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        let color = if key.is_black {
            colors.black
        } else {
            colors.white
        };
        // 像素矩形 → clip 空间（y 向下 → 需翻转），整体下移 y_offset 贴底
        let rx = (key.x / w) * 2.0 - 1.0;
        let ry = 1.0 - (((key.y + y_offset) + key.h) / h) * 2.0;
        let rw = (key.w / w) * 2.0;
        let rh = (key.h / h) * 2.0;
        out.push(Instance {
            rect: [rx, ry, rw, rh],
            color: [color[0], color[1], color[2], color[3]],
        });
    }
    out
}

/// 构建音符 uniform 字节（32 字节，std140 布局，见 `NoteUniforms`）
fn build_uniforms(
    width: f32,
    height: f32,
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
    key_count: u32,
    keyboard_y: f32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    for f in [width, height] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for f in [zoom_x, scroll_x] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for u in [current_track, key_count] {
        out.extend_from_slice(&u.to_le_bytes());
    }
    for f in [keyboard_y, 0.0f32] {
        out.extend_from_slice(&f.to_le_bytes());
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
