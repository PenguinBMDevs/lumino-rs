//! 钢琴瀑布流面板离屏渲染器（键盘底条 + 下落式音符）
//!
//! 将「下落式音符 + 底部钢琴键盘」绘制到一张离屏 `Rgba8Unorm` 纹理，读回 RGBA
//! 字节后交由 iced 以 `image::Handle` 显示。
//!
//! 关键约束（来自需求）：
//! - **音符数据禁止第二份拷贝**——直接 bind 渲染线程已持有的活体 GPU 音符实例缓冲
//!   （只读 `storage`），在面板 offscreen pass 中复用，不重新上传。
//! - **可视区间剔除**：通过一次 compute 预过滤，仅把落在可见纵轴区间的音符索引写入
//!   间接绘制缓冲，主绘制用 `draw_indirect` 只画可见音符——杜绝百万级音符每帧全量顶点。
//! - **不阻塞 UI 线程 / 不闪烁**：纹理与读回缓冲跨帧复用，读回走异步 `map_async` +
//!   非阻塞 `poll`，绝不 `wait_indefinitely`，避免滚动时卡顿与撕裂。
//! - **背景透明**：整张纹理以 alpha=0 清屏（不填白），瀑布流区域透出面板自身背景；
//!   仅音符与键盘键位为不透明像素。

use std::sync::mpsc;
use std::time::Duration;

use iced_wgpu::wgpu;
use wgpu::util::DeviceExt;

use super::key_layout::{self, KeyRect};

/// 键盘渲染配色（0..1 线性）
pub struct KeyboardColors {
    /// 白键填充色
    pub white: [f32; 4],
    /// 黑键填充色
    pub black: [f32; 4],
}

impl KeyboardColors {
    /// 纯黑白配色（白键纯白，黑键纯黑），无边框/缝隙
    pub fn pure() -> Self {
        Self {
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
/// compute 工作组大小
const WORKGROUP_SIZE: u32 = 64;

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
    o.pos = vec4<f32>(rect.xy + pos * rect.zw, 0.0, 1.0);
    o.color = color;
    return o;
}

@fragment
fn fs(@location(0) color: vec4<f32>) -> @location(0) vec4<f32> {
    return color;
}
"#;

/// 下落式音符着色器（复用渲染线程活体 GPU 实例缓冲，只读 storage + 可见索引）
///
/// 与 `onion_note.wgsl` 保持一致的取色逻辑：调色板色（unpack_key_color）+
/// 主音轨蓝覆盖（`border_width >> 16 == current_track`）。
/// 纵向映射：把卷帘的 x 轴旋转 90° 到面板 y 轴——底部键盘线 = 当前时间（min_tick），
/// 后续 tick 自顶部流入、向下落到键盘（瀑布流）。
/// 实例索引经 compute 剔除后由 `visible_indices` 间接给出，仅绘制可见音符。
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
@group(0) @binding(2) var<storage, read> visible_indices: array<u32>;

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
    if (rem >= 1) { wb = wb + 1; }
    if (rem >= 3) { wb = wb + 1; }
    if (rem >= 5) { wb = wb + 1; }
    if (rem >= 6) { wb = wb + 1; }
    if (rem >= 8) { wb = wb + 1; }
    if (rem >= 10) { wb = wb + 1; }
    if (rem >= 11) { wb = wb + 1; }
    return oct * 7 + wb;
}

@vertex
fn vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let cpos = corners[vid];

    let real = visible_indices[iid];
    let inst = notes[real];
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

/// 可视区间剔除 compute：把落在可见纵轴区间的音符索引写入 `visible_indices`，
/// 并以原子自增维护 `draw_args[1]`（间接绘制的 instance_count）。
const CULL_SHADER: &str = r#"
struct NoteInstance {
    start_length: vec2<f32>,
    key_color: u32,
    border_width: u32,
};

struct NoteUniforms {
    resolution: vec2<f32>,
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
    key_count: u32,
    keyboard_y: f32,
    _pad: f32,
};

@group(0) @binding(0) var<storage, read> notes: array<NoteInstance>;
@group(0) @binding(1) var<uniform> u: NoteUniforms;
@group(0) @binding(2) var<storage, read_write> visible_indices: array<u32>;
// 间接绘制参数：[vertex_count, instance_count, first_vertex, first_instance]
@group(0) @binding(3) var<storage, read_write> draw_args: array<atomic<u32>>;
// 分块调度偏移：单次 dispatch 工作群组数上限 65535，超量音符需分块，每块带各自偏移
struct CullOffset { offset: u32, _p0: u32, _p1: u32, _p2: u32, };
@group(0) @binding(4) var<uniform> cull_offset: CullOffset;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x + cull_offset.offset;
    if (i >= arrayLength(&notes)) { return; }
    let inst = notes[i];
    let start = inst.start_length.x;
    let len = inst.start_length.y;
    let y_top = u.keyboard_y - ((start + len) * u.zoom_x - u.scroll_x);
    let y_bot = u.keyboard_y - (start * u.zoom_x - u.scroll_x);
    if (y_bot >= -4.0 && y_top <= u.resolution.y + 4.0) {
        let slot = atomicAdd(&draw_args[1], 1u);
        visible_indices[slot] = i;
    }
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

/// 钢琴瀑布流离屏渲染器（持有管线与单位四边形，跨帧复用纹理/缓冲）
pub struct KeyboardRenderer {
    /// 键盘底条管线
    pipeline: wgpu::RenderPipeline,
    /// 单位四边形顶点缓冲
    quad_buffer: wgpu::Buffer,
    /// 下落式音符管线（间接绘制）
    note_pipeline: wgpu::RenderPipeline,
    /// 音符 bind group layout（只读 storage 实例 + uniform + 可见索引）
    note_bind_group_layout: wgpu::BindGroupLayout,
    /// 音符 uniform 缓冲（每帧 write_buffer 更新）
    uniform_buffer: wgpu::Buffer,
    /// 可视区间剔除 compute 管线
    cull_pipeline: wgpu::ComputePipeline,
    /// 剔除 bind group layout（notes + uniforms + visible_indices + draw_args）
    cull_bind_group_layout: wgpu::BindGroupLayout,
    /// 可见音符索引缓冲（storage，容量随音符数变化）
    visible_indices: wgpu::Buffer,
    /// 间接绘制参数缓冲 [vertex_count, instance_count, first_vertex, first_instance]
    draw_args: wgpu::Buffer,
    /// 离屏目标纹理（跨帧复用，尺寸变化才重建）
    tex: Option<wgpu::Texture>,
    tex_view: Option<wgpu::TextureView>,
    /// 读回缓冲（跨帧复用；同一时刻仅一个读回在途）
    staging: Option<wgpu::Buffer>,
    /// 读回完成通道（非空 RGBA + 宽高）
    readback_tx: mpsc::Sender<(Vec<u8>, u32, u32)>,
    readback_rx: mpsc::Receiver<(Vec<u8>, u32, u32)>,
    /// 是否有读回在途（在途期间不再发起新读回，避免映射同一缓冲）
    map_pending: bool,
    /// 已分配的纹理/缓冲尺寸与音符数（用于判定是否需要重建）
    last_w: u32,
    last_h: u32,
    last_count: u32,
    /// 离屏纹理每行对齐后的字节数（宽度向上取整到 64 的倍数）
    padded_width: u32,
}

impl KeyboardRenderer {
    /// 创建渲染器（键盘 + 音符 + 剔除 三套管线，及复用缓冲）
    pub fn new(device: &wgpu::Device) -> Self {
        let keyboard_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("piano_waterfall_keyboard"),
            source: wgpu::ShaderSource::Wgsl(KEYBOARD_SHADER.into()),
        });
        let note_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("piano_waterfall_note"),
            source: wgpu::ShaderSource::Wgsl(NOTE_SHADER.into()),
        });
        let cull_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("piano_waterfall_cull"),
            source: wgpu::ShaderSource::Wgsl(CULL_SHADER.into()),
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
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

        let cull_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("piano_waterfall_cull_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let cull_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("piano_waterfall_cull"),
                bind_group_layouts: &[&cull_bind_group_layout],
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

        let cull_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("piano_waterfall_cull"),
                layout: Some(&cull_pipeline_layout),
                module: &cull_shader,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
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

        // 可见索引缓冲（容量随音符数变化，初始 1 避免 0 尺寸）
        let visible_indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_visible_indices"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        // 间接绘制参数缓冲：vertex_count=6, instance_count=0, first=0, first=0
        let draw_args = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_draw_args"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (readback_tx, readback_rx) = mpsc::channel();

        Self {
            pipeline: keyboard_pipeline,
            quad_buffer,
            note_pipeline,
            note_bind_group_layout,
            uniform_buffer,
            cull_pipeline,
            cull_bind_group_layout,
            visible_indices,
            draw_args,
            tex: None,
            tex_view: None,
            staging: None,
            readback_tx,
            readback_rx,
            map_pending: false,
            last_w: 0,
            last_h: 0,
            last_count: 0,
            padded_width: 0,
        }
    }

    /// 是否仍有读回在途（供 Host 决定是否续帧轮询，避免空闲时卡死 / 活跃时撕裂）
    pub(crate) fn is_readback_pending(&self) -> bool {
        self.map_pending
    }

    /// 确保离屏纹理与读回缓冲尺寸匹配（跨帧复用，仅在尺寸变化时重建）
    fn ensure_targets(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.last_w == width && self.last_h == height && self.tex.is_some() {
            return;
        }
        // 尺寸变化且仍有读回在途：先让在途读回完成，下一帧再重建，避免 drop 已映射缓冲
        if self.map_pending {
            return;
        }
        let padded_width = (width + 63) & !63;
        self.padded_width = padded_width;
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
        let tex_view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_staging"),
            size: (padded_width * 4 * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.tex = Some(tex);
        self.tex_view = Some(tex_view);
        self.staging = Some(staging);
        self.last_w = width;
        self.last_h = height;
    }

    /// 确保可见索引缓冲容量匹配音符数（仅在数量变化时重建）
    fn ensure_visible_indices(&mut self, device: &wgpu::Device, count: u32) {
        if self.last_count == count && count > 0 {
            return;
        }
        if self.map_pending {
            return; // 在途读回期间不重建缓冲
        }
        let size = (count.max(1) as u64) * 4;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_visible_indices"),
            size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        self.visible_indices = buf;
        self.last_count = count;
    }

    /// 渲染「下落式音符 + 底部键盘」到离屏纹理并异步读回 RGBA 字节。
    ///
    /// 返回 `Some((rgba, w, h))` 表示本次拿到一帧读回结果（调用方据此更新 `image::Handle`）；
    /// 返回 `None` 表示读回在途或资源正在重建，应保持旧 Handle 下一帧重试。
    ///
    /// - `note_data`：渲染线程发布的活体 GPU 实例缓冲与实例数；`None` 时仅渲染键盘。
    /// - `zoom_x` / `scroll_x`：与钢琴卷帘 X 缩放/滚动一致，驱动音符落点与时间流。
    /// - `current_track`：主音轨编码（`current_track_idx + 1`），用于蓝色覆盖。
    #[allow(clippy::too_many_arguments)]
    pub fn render_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        key_count: u32,
        note_data: Option<(wgpu::Buffer, u32)>,
        zoom_x: f32,
        scroll_x: f32,
        current_track: u32,
    ) -> Option<(Vec<u8>, u32, u32)> {
        let width = width.max(1);
        let height = height.max(1);
        let count = note_data.as_ref().map(|(_, c)| *c).unwrap_or(0);

        // 非阻塞轮询：驱动在途的 map_async 回调完成（不依赖渲染线程，避免读回永远卡住）
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_millis(0)),
        });

        // 1) 尝试收集已完成的读回（非阻塞）
        if let Ok((bytes, w, h)) = self.readback_rx.try_recv() {
            self.map_pending = false;
            // 空字节 = 读回失败哨兵：放弃本帧，下一帧重试
            if bytes.is_empty() {
                return None;
            }
            return Some((bytes, w, h));
        }
        // 2) 仍有读回在途：保持旧 Handle，不发起新读回
        if self.map_pending {
            return None;
        }

        // 键盘底条：高度按宽度比例联动，贴底
        let kb_h = (width as f32 * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
        let keyboard_y = height as f32 - kb_h;

        self.ensure_targets(device, width, height);
        self.ensure_visible_indices(device, count);
        let tex = self.tex.as_ref()?;
        let tex_view = self.tex_view.as_ref()?;
        let staging = self.staging.as_ref()?;

        let colors = KeyboardColors::pure();
        let mut keys = key_layout::build_layout(width as f32, kb_h, key_count);
        keys.sort_by_key(|k| k.is_black); // 白键在前、黑键在后，黑键覆盖白键
        let instances = build_instances(width, height as f32, keyboard_y, &keys, &colors);
        let instance_bytes = instances_to_bytes(&instances);
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("piano_waterfall_keyboard_instances"),
            contents: &instance_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });

        // 音符 uniform（含落点线 keyboard_y）
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

        // 间接绘制参数：vertex_count=6，instance_count 由 compute 原子自增
        let draw_args_bytes = {
            let mut db = Vec::with_capacity(16);
            for u in [6u32, 0u32, 0u32, 0u32] {
                db.extend_from_slice(&u.to_le_bytes());
            }
            db
        };
        queue.write_buffer(&self.draw_args, 0, &draw_args_bytes);

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        // 可视区间剔除：仅把可见音符索引写入 visible_indices + draw_args[1]
        // 超量音符（>65535 工作群组）分块调度，每块带各自 dispatch 偏移，
        // 仍累加到同一 draw_args 原子计数，主绘制只画可见音符。
        if let Some((buf, c)) = &note_data
            && *c > 0
        {
            let total_wg = (*c).div_ceil(WORKGROUP_SIZE);
            let max_wg = 65535u32;
            let mut cp = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("piano_waterfall_cull"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&self.cull_pipeline);
            let mut dispatched = 0u32;
            while dispatched < total_wg {
                let wg_count = (total_wg - dispatched).min(max_wg);
                let offset = dispatched * WORKGROUP_SIZE;
                let mut ob = Vec::with_capacity(16);
                ob.extend_from_slice(&offset.to_le_bytes());
                ob.extend_from_slice(&[0u8; 12]);
                let off_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("piano_waterfall_cull_offset"),
                    contents: &ob,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let cull_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("piano_waterfall_cull_bg"),
                    layout: &self.cull_bind_group_layout,
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
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(
                                self.visible_indices.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(
                                self.draw_args.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Buffer(
                                off_buf.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
                cp.set_bind_group(0, &cull_bg, &[]);
                cp.dispatch_workgroups(wg_count, 1, 1);
                dispatched += wg_count;
            }
        }

        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("piano_waterfall_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: tex_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 透明清屏：不填白，瀑布流区域透出面板背景
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 1) 下落式音符（仅可见音符，间接绘制；复用渲染线程活体 GPU 实例缓冲）
            if let Some((buf, _)) = &note_data
                && count > 0
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
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(
                                self.visible_indices.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
                rp.set_pipeline(&self.note_pipeline);
                rp.set_bind_group(0, &bg_note, &[]);
                rp.draw_indirect(&self.draw_args, 0);
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
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_width * 4),
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

        // 3) 发起异步读回（非阻塞）：克隆两份缓冲引用，避免移动与借用冲突
        let staging_for_map = staging.clone();
        let staging_for_cb = staging.clone();
        let tx = self.readback_tx.clone();
        // 注意：行跨度必须是**字节**数 = padded_width * 4（RGBA），少乘 4 会把图像按 4 段错位缠绕
        let (w, h, padded, active) = (width, height, self.padded_width * 4, width * 4);
        staging_for_map.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            if res.is_ok() {
                let slice = staging_for_cb.slice(..);
                let mapped = slice.get_mapped_range();
                let row = padded as usize;
                let active = active as usize;
                let mut rgba = Vec::with_capacity(active * h as usize);
                for y in 0..h as usize {
                    let start = y * row;
                    rgba.extend_from_slice(&mapped[start..start + active]);
                }
                drop(mapped);
                staging_for_cb.unmap();
                let _ = tx.send((rgba, w, h));
            } else {
                // 读回失败哨兵（空字节），调用方据此跳过本帧
                let _ = tx.send((Vec::new(), w, h));
            }
        });
        // 非阻塞轮询：驱动 GPU 完成拷贝与映射回调，绝不阻塞 UI 线程
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_millis(0)),
        });
        // 标记读回在途：在途期间不再向同一缓冲发起新读回/拷贝，避免映射冲突
        self.map_pending = true;
        // 极可能尚未完成：本帧不更新 Handle，下一帧收集
        None
    }
}

/// 像素矩形 → clip 空间实例数据（白键 → 黑键排序），并整体下移 `y_offset` 贴底
///
/// 注意：`full_height` 为**整张纹理高度**（用于 clip 空间归一化），与键条局部高度不同。
fn build_instances(
    width: u32,
    full_height: f32,
    y_offset: f32,
    keys: &[KeyRect],
    colors: &KeyboardColors,
) -> Vec<Instance> {
    let w = width as f32;
    let h = full_height;
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
