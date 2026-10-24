// 像素聚合渲染（splat）：全景放大档把「每音符 1 个点图元」降为
// 「每屏幕像素 1 个胜者」，再由一次全屏合成输出。
//
// 语义与 `onion_note.wgsl::vs_point` 的硬件深度测试一致：
// - 每个音符仍被处理（不 LOD、不裁掉可见音符）；
// - 同一像素取「主音轨优先、否则源索引最小」的音符（对应区域深度序）；
// - 颜色 = 该音符的实例色 / 主音轨蓝；静音轨不参与。
//
// 为什么需要它：RTX 2060 实测点图元约 0.7ns/实例固定成本，64M 音符 ≈ 45ms；
// 本路径把 64M 点 → 64M compute 线程 + 2M 像素合成，实测 compute ≈ 12ms。

const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);
const MAIN_DEPTH_REGION_BITS: u32 = 0x00800000u;
const ONION_DEPTH_REGION_BITS: u32 = 0x1F800000u;
const DEPTH_REGION_SLOTS: u32 = 0x1F000000u;

struct CameraUniform {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    canvas_size: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: vec2<f32>,
}

struct ViewState {
    current_track: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    muted_bits: array<vec4<u32>, 512>,
}

// ── 12B note layout（16B wasabi 布局已删除）────────────────────────────
struct NoteInstance {
    start: f32,
    length: f32,
    attrs: u32, // key(0..7) | track_enc(8..23) | border(24..30) | preview(31)
};

fn note_key(attrs: u32) -> u32 { return attrs & 0xFFu; }
fn note_track(attrs: u32) -> u32 { return (attrs >> 8u) & 0xFFFFu; }
fn note_border(attrs: u32) -> u32 { return (attrs >> 24u) & 0x7Fu; }
fn note_is_preview(attrs: u32) -> bool { return (attrs & 0x80000000u) != 0u; }

// 与 lumino_extras fallback 调色板同色；GPU 调色板缓冲接入前的兜底。
const FALLBACK_TRACK_COLORS: array<vec4<f32>, 12> = array<vec4<f32>, 12>(
    vec4<f32>(0.784, 0.314, 0.314, 1.0),
    vec4<f32>(0.314, 0.784, 0.471, 1.0),
    vec4<f32>(0.314, 0.471, 0.863, 1.0),
    vec4<f32>(0.863, 0.784, 0.314, 1.0),
    vec4<f32>(0.784, 0.392, 0.784, 1.0),
    vec4<f32>(0.314, 0.784, 0.784, 1.0),
    vec4<f32>(0.941, 0.588, 0.314, 1.0),
    vec4<f32>(0.706, 0.706, 0.706, 1.0),
    vec4<f32>(0.902, 0.392, 0.392, 1.0),
    vec4<f32>(0.392, 0.706, 0.392, 1.0),
    vec4<f32>(0.392, 0.392, 0.784, 1.0),
    vec4<f32>(0.784, 0.706, 0.392, 1.0),
);

fn note_color(attrs: u32) -> vec4<f32> {
    let idx = max(note_track(attrs), 1u) - 1u;
    return FALLBACK_TRACK_COLORS[idx % 12u];
}/// 物理像素尺寸 + 逻辑→物理缩放（X/Y 分开，兼容非 1 的 DPI/视口取整）。
struct SplatParams {
    width: u32,
    height: u32,
    scale_x: f32,
    scale_y: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> view_state: ViewState;
@group(0) @binding(2) var<storage, read> all_instances: array<NoteInstance>;
@group(0) @binding(3) var<storage, read_write> depth: array<atomic<u32>>;
@group(0) @binding(4) var<uniform> params: SplatParams;

fn is_muted_track(track_enc: u32) -> bool {
    if (track_enc == 0u) {
        return false;
    }
    let track_idx = track_enc - 1u;
    let word = track_idx / 32u;
    let bit = track_idx % 32u;
    if (word >= 2048u) {
        return false;
    }
    let v = view_state.muted_bits[word / 4u];
    return ((v[word % 4u] >> bit) & 1u) != 0u;
}

fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

/// 每个音符算出屏幕像素，用 atomicMax 保留「最近的音符」：
/// packed = 主音轨位(31) | (0x7FFFFFFF - 源索引)，值越大越近/越小索引。
@compute @workgroup_size(256)
fn splat(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x + gid.y * 65535u * 256u;
    if (i >= arrayLength(&all_instances)) {
        return;
    }
    let n = all_instances[i];
    let track_enc = note_track(n.attrs);
    let is_main = track_enc == view_state.current_track;
    if (!is_main && is_muted_track(track_enc)) {
        return;
    }
    let length = n.length;
    if (length <= 0.0) {
        return;
    }
    let key = f32(note_key(n.attrs));
    // 与 `vs_point` **逐运算顺序一致**：先算起点，再加半长/半高。
    // 若先算 `(tick + length*0.5)`，大 tick（黑乐谱可达 1e9）下 f32 会把
    // length/2 吃掉，导致落点与点图元差出整像素。
    let screen_w = length * camera.zoom.x;
    let base_x = n.start * camera.zoom.x - camera.scroll.x
        + camera.keyboard_width + camera.canvas_offset.x;
    let base_y = (camera.max_key_index - key) * camera.zoom.y
        - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    let center_x = base_x + screen_w * 0.5;
    let center_y = base_y + camera.zoom.y * 0.5;
    // 像素落点必须与硬件点图元一致：1px 点覆盖 `[c-0.5, c+0.5]`，
    // 硬件实际取**左/上**格，等价规则是 `floor(c - 0.5)`。
    // 实测：100.5→100、111.0→110、160.00032→159（不是 ceil-1 的 160）。
    // 非整数时 `floor(c)` 只在小数 ≥0.5 时巧合成立，不能替代。
    let px = floor(center_x * params.scale_x - 0.5);
    let py = floor(center_y * params.scale_y - 0.5);
    if (px < 0.0 || py < 0.0 || px >= f32(params.width) || py >= f32(params.height)) {
        return;
    }
    let idx = u32(py) * params.width + u32(px);
    let packed = select(0u, 0x80000000u, is_main) | (0x7FFFFFFFu - i);
    _ = atomicMax(&depth[idx], packed);
}

/// 全屏合成：只对胜者像素写颜色/深度；空格 discard（保留背景/网格）。
struct SplatOut {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // 覆盖全屏的大三角形（比两三角形少一次顶点/光栅边界）
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[vi], 0.0, 1.0);
}

@fragment
fn fs_splat(@builtin(position) pos: vec4<f32>) -> SplatOut {
    let x = u32(pos.x);
    let y = u32(pos.y);
    if (x >= params.width || y >= params.height) {
        discard;
    }
    let packed = depth[y * params.width + x];
    if (packed == 0u) {
        discard;
    }
    let is_main = (packed & 0x80000000u) != 0u;
    let inv = packed & 0x7FFFFFFFu;
    let index = 0x7FFFFFFFu - inv;
    let inst = all_instances[index];
    var color = note_color(inst.attrs);
    if (is_main) {
        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
    }
    let region_bits = select(ONION_DEPTH_REGION_BITS, MAIN_DEPTH_REGION_BITS, is_main);
    let depth_bits = region_bits + min(index, DEPTH_REGION_SLOTS - 1u);
    var out: SplatOut;
    out.color = color;
    out.depth = bitcast<f32>(depth_bits);
    return out;
}
