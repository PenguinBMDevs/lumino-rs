//! 钢琴瀑布流 WGSL 着色器常量（键盘底条 / 下落音符 / 活跃键颜色 / 可视区间剔除）

/// 键盘底条着色器（单位四边形 + 实例矩形 + 活跃键颜色混合）
///
/// 活跃键颜色 `key_colors[key]` 复用钢琴卷帘瀑布流（`gfx::waterfall.wgsl`）的编码：
/// `0xRRGGBBAA`（0 表示无高亮），混合算法 `blend_key_color` 逐字移植自同一文件，
/// 保证面板键盘与主卷帘键盘高亮观感完全一致。
pub(super) const KEYBOARD_SHADER: &str = r#"
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
pub(super) const NOTE_SHADER: &str = r#"
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
pub(super) const KEYCOLOR_SHADER: &str = r#"
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
pub(super) const CULL_SHADER: &str = r#"
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
