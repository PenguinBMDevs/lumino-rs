//! 钢琴瀑布流面板离屏渲染器（键盘底条 + 下落式音符）
//!
//! 将「下落式音符 + 底部钢琴键盘」绘制到一张离屏 `Rgba8Unorm` 纹理（`self.tex`），
//! 其纹理视图由 iced 的 `shader` 图元在自身渲染通道内**直接采样合成**（GPU→GPU），
//! 不经过 CPU 读回、不经 `image::Handle`、不进 iced 图集——因此与钢琴卷帘洋葱皮同样不闪烁。
//!
//! 关键约束（来自需求）：
//! - **音符数据禁止第二份拷贝**——直接 bind 渲染线程已持有的活体 GPU 音符实例缓冲
//!   （只读 `storage`），在面板 offscreen pass 中复用，不重新上传。
//! - **可视区间剔除**：通过一次 compute 预过滤，仅把落在可见纵轴区间的音符索引写入
//!   间接绘制缓冲，主绘制用 `draw_indirect` 只画可见音符——杜绝百万级音符每帧全量顶点。
//! - **背景透明**：整张纹理以 alpha=0 清屏（不填白），瀑布流区域透出面板自身背景；
//!   仅音符与键盘键位为不透明像素。

use std::sync::Arc;

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
/// 活跃键颜色缓冲支持的最大键数（覆盖 128 与 256 键两种模式）
const KEY_COUNT_MAX: u32 = 256;
/// 每帧清零活跃键颜色用的零缓冲（KEY_COUNT_MAX × u32）
const ZERO_KEYCOLORS: [u8; (KEY_COUNT_MAX as usize) * 4] = [0u8; (KEY_COUNT_MAX as usize) * 4];

/// 键盘底条着色器（单位四边形 + 实例矩形 + 活跃键颜色混合）
///
/// 活跃键颜色 `key_colors[key]` 复用钢琴卷帘瀑布流（`gfx::waterfall.wgsl`）的编码：
/// `0xRRGGBBAA`（0 表示无高亮），混合算法 `blend_key_color` 逐字移植自同一文件，
/// 保证面板键盘与主卷帘键盘高亮观感完全一致。
const KEYBOARD_SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// 活跃键颜色（复用卷帘瀑布流的 0xRRGGBBAA 打包：与 gfx waterfall.wgsl 一致）
@group(0) @binding(0) var<storage, read> key_colors: array<u32>;

// 解包 0xRRGGBBAA（R 在高字节，A 在低字节），与 gfx waterfall.wgsl 的 unpack_color 一致
fn unpack_kc(packed: u32) -> vec4<u32> {
    let r = (packed >> 24u) & 0xFFu;
    let g = (packed >> 16u) & 0xFFu;
    let b = (packed >> 8u) & 0xFFu;
    let a = packed & 0xFFu;
    return vec4<u32>(r, g, b, a);
}

// 复用卷帘瀑布流的 blend_key_color：base 为底色键色，overlay 为活跃色，alpha 控制混合强度
fn blend_key_color(base: vec4<u32>, overlay: vec4<u32>, alpha: u32) -> vec4<u32> {
    if (overlay.a == 0u || alpha == 0u) {
        return base;
    }
    let a = alpha;
    let r = (base.x * (255u - a) + overlay.x * a) / 255u;
    let g = (base.y * (255u - a) + overlay.y * a) / 255u;
    let b = (base.z * (255u - a) + overlay.z * a) / 255u;
    return vec4<u32>(r, g, b, 255u);
}

@vertex
fn vs(
    @location(0) pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) key: u32,
) -> VsOut {
    var o: VsOut;
    o.pos = vec4<f32>(rect.xy + pos * rect.zw, 0.0, 1.0);

    var out_color = color;
    let ki = min(key, 255u);
    let ac = key_colors[ki];
    if (ac != 0u) {
        let overlay = unpack_kc(ac);
        let base8 = vec4<u32>(
            u32(clamp(color.r, 0.0, 1.0) * 255.0),
            u32(clamp(color.g, 0.0, 1.0) * 255.0),
            u32(clamp(color.b, 0.0, 1.0) * 255.0),
            255u,
        );
        let blended = blend_key_color(base8, overlay, overlay.a);
        out_color = vec4<f32>(f32(blended.x), f32(blended.y), f32(blended.z), 255.0) / 255.0;
    }
    o.color = out_color;
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
//
// 阈值 {1,3,5,6,8,10} 为黑键音级：跨过一个黑键阈值，白键序号 +1。
// 注意：B（rem=11）是**白键**，绝不能计入阈值——曾误加 `rem >= 11` 分支，
// 导致每个八度的 B 调白键序号多算 1，瀑布流音符右移一个白键宽、错位到下一八度 C 列，
// 表现为“B 调音符不显示”。此处仅统计真正落在 [0, rem) 的白键，故不含 rem=11。
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

/// 活跃键颜色 compute：把「正跨过键盘线（落键）」的音符对应的键颜色写入 `key_colors`。
///
/// 键盘线 tick = `scroll_x / zoom_x`（面板底部键盘落点线），与 NOTE_SHADER 的落点一致；
/// 音符 tick 区间覆盖该线即视为「正在落键」，点亮其键。颜色复用 NOTE_SHADER 的取色逻辑
/// （`unpack_key_color` + 主音轨蓝覆盖），并打包为卷帘瀑布流的 `0xRRGGBBAA` 格式，
/// 由键盘着色器的 `blend_key_color` 混合——与主卷帘键盘高亮完全同源。
const KEYCOLOR_SHADER: &str = r#"
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
@group(0) @binding(2) var<storage, read_write> key_colors: array<u32>;
// 分块调度偏移：单次 dispatch 工作群组数上限 65535，超量音符需分块，每块带各自偏移
struct CullOffset { offset: u32, _p0: u32, _p1: u32, _p2: u32, };
@group(0) @binding(3) var<uniform> cull_offset: CullOffset;

const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);
// 活跃键高亮强度（0..255），复用卷帘瀑布流 0xRRGGBBAA 打包
const ACTIVE_KEY_ALPHA: u32 = 200u;

fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

fn pack_kc(r: u32, g: u32, b: u32, a: u32) -> u32 {
    return (r << 24u) | (g << 16u) | (b << 8u) | a;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x + cull_offset.offset;
    if (i >= arrayLength(&notes)) { return; }
    let inst = notes[i];
    let k = inst.key_color & 0xFFu;
    // 键盘线 tick：面板底部键盘线 = 当前滚动线（scroll_x / zoom_x）。
    // 音符区间覆盖该线即正“落键”，点亮对应键。
    let tk = u.scroll_x / u.zoom_x;
    let start = inst.start_length.x;
    let len = inst.start_length.y;
    if (tk >= start && tk <= start + len) {
        var col = unpack_key_color(inst.key_color);
        if ((inst.border_width >> 16u) == u.current_track) {
            col = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
        }
        let r = u32(clamp(col.r, 0.0, 1.0) * 255.0);
        let g = u32(clamp(col.g, 0.0, 1.0) * 255.0);
        let b = u32(clamp(col.b, 0.0, 1.0) * 255.0);
        key_colors[k] = pack_kc(r, g, b, ACTIVE_KEY_ALPHA);
    }
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

/// 单个实例数据：clip 空间矩形（xy=min, zw=size）+ 填充色 + 键号（索引活跃键颜色）
#[repr(C)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    key: u32,
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
    /// 活跃键颜色缓冲（storage，KEY_COUNT_MAX × u32，packed 0xRRGGBBAA；每帧清零后由 compute 写入）
    key_colors: Option<wgpu::Buffer>,
    /// 活跃键颜色 compute 管线
    keycolor_pipeline: wgpu::ComputePipeline,
    /// 活跃键颜色 compute bind group layout（notes + uniforms + key_colors + cull_offset）
    keycolor_bind_group_layout: wgpu::BindGroupLayout,
    /// 键盘着色器 bind group layout（key_colors storage read，仅顶点阶段）
    key_bgl: wgpu::BindGroupLayout,
    /// 离屏目标纹理（跨帧复用，尺寸变化才重建）；其视图交给 iced shader 图元直接采样合成
    tex: Option<wgpu::Texture>,
    /// `tex` 的视图（跨帧复用）；iced shader 图元持有其 `Arc` 克隆，在自身渲染通道内采样。
    /// 用 `Arc` 包裹以便跨帧/跨图元共享，且纹理重建时旧视图仍可被在途图元安全引用。
    tex_view: Option<Arc<wgpu::TextureView>>,
    /// 已分配的纹理/缓冲尺寸与音符数（用于判定是否需要重建）
    last_w: u32,
    last_h: u32,
    last_count: u32,
    /// 键盘实例缓冲（持久化复用）
    ///
    /// 键盘几何仅依赖 `width/height/key_count`，与滚动/缩放无关；旧实现每帧
    /// `create_buffer_init` 新建并立即 drop，触发驱动延迟释放，在播放自动滚动
    /// （scroll_x 每帧变化→签名变化→每帧 render_scene）时造成突发性 GPU 尖刺。
    /// 现跨帧复用，三者任一变化时才重建。
    instance_buffer: Option<wgpu::Buffer>,
    /// 实例缓冲对应的尺寸/键数（用于判定是否需要重建）
    inst_w: u32,
    inst_h: u32,
    inst_keys: u32,
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

        // 键盘着色器 bind group layout：活跃键颜色（只读 storage，仅顶点阶段）
        let key_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("piano_waterfall_key_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let keyboard_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("piano_waterfall_keyboard"),
                bind_group_layouts: &[&key_bgl],
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
        let note_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
        let cull_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("piano_waterfall_cull"),
            bind_group_layouts: &[&cull_bind_group_layout],
            push_constant_ranges: &[],
        });

        let keyboard_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Uint32,
                                offset: 32,
                                shader_location: 3,
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

        let cull_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("piano_waterfall_cull"),
            layout: Some(&cull_pipeline_layout),
            module: &cull_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // 活跃键颜色 compute bind group layout（notes + uniforms + key_colors + cull_offset）
        let keycolor_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("piano_waterfall_keycolor_bgl"),
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
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let keycolor_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("piano_waterfall_keycolor"),
                bind_group_layouts: &[&keycolor_bind_group_layout],
                push_constant_ranges: &[],
            });
        let keycolor_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("piano_waterfall_keycolor"),
            source: wgpu::ShaderSource::Wgsl(KEYCOLOR_SHADER.into()),
        });
        let keycolor_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("piano_waterfall_keycolor"),
            layout: Some(&keycolor_pipeline_layout),
            module: &keycolor_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // 单位正方形 [0,1]^2 → 两个三角形（6 顶点）
        let quad: [f32; 12] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
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
            key_colors: None,
            keycolor_pipeline,
            keycolor_bind_group_layout,
            key_bgl,
            tex: None,
            tex_view: None,
            last_w: 0,
            last_h: 0,
            last_count: 0,
            instance_buffer: None,
            inst_w: 0,
            inst_h: 0,
            inst_keys: 0,
        }
    }

    /// 确保活跃键颜色缓冲存在（KEY_COUNT_MAX × u32，storage + 每帧清零用 COPY_DST）
    fn ensure_key_colors(&mut self, device: &wgpu::Device) {
        if self.key_colors.is_some() {
            return;
        }
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_key_colors"),
            size: (KEY_COUNT_MAX as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.key_colors = Some(buf);
    }

    /// 确保离屏纹理尺寸匹配（跨帧复用，仅在尺寸变化时重建）；其视图交给 iced shader 图元采样。
    fn ensure_targets(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.last_w == width && self.last_h == height && self.tex.is_some() {
            return;
        }
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
            // RENDER_ATTACHMENT：离屏渲染目标；TEXTURE_BINDING：iced shader 图元采样合成。
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let tex_view = Arc::new(tex.create_view(&wgpu::TextureViewDescriptor::default()));
        self.tex = Some(tex);
        self.tex_view = Some(tex_view);
        self.last_w = width;
        self.last_h = height;
    }

    /// 确保可见索引缓冲容量匹配音符数（仅在数量变化时重建）
    fn ensure_visible_indices(&mut self, device: &wgpu::Device, count: u32) {
        if self.last_count == count && count > 0 {
            return;
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

    /// 渲染「下落式音符 + 底部键盘」到离屏纹理，返回其纹理视图供 iced `shader` 图元直接采样合成。
    ///
    /// 返回 `Some(Arc<TextureView>)` 表示本次渲染完成（调用方据此更新面板持有的视图）；
    /// 返回 `None` 表示离屏资源尚未就绪（尺寸/缓冲尚未分配），下一帧重试即可。
    ///
    /// 注意：**不做 CPU 读回**。纹理由 iced 在自身渲染通道内直接采样，GPU→GPU 合成，
    /// 与钢琴卷帘洋葱皮同一路径，因此不进 `image::Handle`、不进 iced 图集、不闪烁。
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
    ) -> Option<Arc<wgpu::TextureView>> {
        puffin::profile_scope!("kbrd_render_scene");
        let width = width.max(1);
        let height = height.max(1);
        let count = note_data.as_ref().map(|(_, c)| *c).unwrap_or(0);

        // 键盘底条：高度按宽度比例联动，贴底
        let kb_h = (width as f32 * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
        let keyboard_y = height as f32 - kb_h;

        self.ensure_targets(device, width, height);
        self.ensure_visible_indices(device, count);
        self.ensure_key_colors(device);
        let tex_view = self.tex_view.as_ref()?;

        // 活跃键颜色缓冲：每帧先清零，再由 keycolor compute 写入“正落键”的键
        let key_colors_buf = self
            .key_colors
            .as_ref()
            .expect("key_colors allocated by ensure_key_colors");
        queue.write_buffer(key_colors_buf, 0, &ZERO_KEYCOLORS);

        let colors = KeyboardColors::pure();
        let mut keys = key_layout::build_layout(width as f32, kb_h, key_count);
        keys.sort_by_key(|k| k.is_black); // 白键在前、黑键在后，黑键覆盖白键
        let instances = build_instances(width, height as f32, keyboard_y, &keys, &colors);

        // 复用持久化实例缓冲：键盘几何仅依赖 (width, height, key_count)，
        // 与滚动/缩放/播放进度无关，逐帧重建 + drop 会触发驱动延迟释放，
        // 在播放自动滚动时造成突发性尖刺。仅当三者变化时才新建。
        if self.instance_buffer.is_none()
            || self.inst_w != width
            || self.inst_h != height
            || self.inst_keys != key_count
        {
            let instance_bytes = instances_to_bytes(&instances);
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("piano_waterfall_keyboard_instances"),
                contents: &instance_bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.instance_buffer = Some(buf);
            self.inst_w = width;
            self.inst_h = height;
            self.inst_keys = key_count;
        }
        let instance_buffer = self
            .instance_buffer
            .as_ref()
            .expect("instance_buffer allocated above");

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

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

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
                            resource: wgpu::BindingResource::Buffer(buf.as_entire_buffer_binding()),
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

        // 活跃键颜色 compute：把“正跨过键盘线（落键）”的音符对应键颜色写入 key_colors。
        // 与剔除同分块策略（>65535 工作群组分块），复用同一条 notes 缓冲与 uniform。
        if let Some((buf, c)) = &note_data
            && *c > 0
        {
            let total_wg = (*c).div_ceil(WORKGROUP_SIZE);
            let max_wg = 65535u32;
            let mut cp2 = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("piano_waterfall_keycolor"),
                timestamp_writes: None,
            });
            cp2.set_pipeline(&self.keycolor_pipeline);
            let mut dispatched = 0u32;
            while dispatched < total_wg {
                let wg_count = (total_wg - dispatched).min(max_wg);
                let offset = dispatched * WORKGROUP_SIZE;
                let mut ob = Vec::with_capacity(16);
                ob.extend_from_slice(&offset.to_le_bytes());
                ob.extend_from_slice(&[0u8; 12]);
                let off_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("piano_waterfall_keycolor_offset"),
                    contents: &ob,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let kc_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("piano_waterfall_keycolor_bg"),
                    layout: &self.keycolor_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(buf.as_entire_buffer_binding()),
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
                                key_colors_buf.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(
                                off_buf.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
                cp2.set_bind_group(0, &kc_bg, &[]);
                cp2.dispatch_workgroups(wg_count, 1, 1);
                dispatched += wg_count;
            }
        }

        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("piano_waterfall_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: tex_view.as_ref(),
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
                            resource: wgpu::BindingResource::Buffer(buf.as_entire_buffer_binding()),
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
            // 绑定活跃键颜色缓冲：键盘着色器据此混合“落键”高亮（复用卷帘瀑布流配色）
            let bg_key = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("piano_waterfall_key_bg"),
                layout: &self.key_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        key_colors_buf.as_entire_buffer_binding(),
                    ),
                }],
            });
            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &bg_key, &[]);
            rp.set_vertex_buffer(0, self.quad_buffer.slice(..));
            rp.set_vertex_buffer(1, instance_buffer.slice(..));
            let kcount = instances.len() as u32;
            if kcount > 0 {
                rp.draw(0..6, 0..kcount);
            }
        }

        queue.submit(std::iter::once(encoder.finish()));

        // 不做 CPU 读回：直接返回离屏纹理视图，由 iced `shader` 图元在自身渲染通道内采样合成
        // （GPU→GPU）。返回克隆的 `Arc`，即使后续纹理重建，旧视图仍可被在途图元安全引用。
        self.tex_view.clone()
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
            key: key.key,
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
    let mut out = Vec::with_capacity(instances.len() * 36);
    for inst in instances {
        for f in inst.rect.iter() {
            out.extend_from_slice(&f.to_le_bytes());
        }
        for f in inst.color.iter() {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out.extend_from_slice(&inst.key.to_le_bytes());
    }
    out
}
