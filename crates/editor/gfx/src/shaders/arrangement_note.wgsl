// 走带音符渲染着色器 — 复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存）
//
// 顶点输入不再是完整 NoteInstance，而是 cull 阶段输出的 u32 源索引
// （instance-step，每 4 顶点一个实例）；VS 从 all_instances storage buffer
// 回查原实例，按 border_width 高 16 位还原文档音轨 → lane_index 得到泳道，
// 计算屏幕位置。GPU 裁剪（cull.wgsl）已剔除视口外音符，本着色器只负责绘制。

const BORDER_DARKEN_FACTOR: f32 = 0.4;

struct Uniforms {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    lane_height: f32,
    note_height: f32,
    _pad: vec2<f32>,
};

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
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

// lane_index[doc_track] = 泳道序号（与 CPU 端 build_arrangement_note_data 一致）
@group(0) @binding(1)
var<storage, read> lane_index: array<f32>;

// 全部音符实例数据（只读 storage，与 cull 阶段读取同一份）
@group(0) @binding(2)
var<storage, read> all_instances: array<NoteInstance>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) screen_size: vec2<f32>,
    @location(3) border_width: f32,
};

fn unpack_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) visible_index: u32,
) -> VertexOutput {
    let instance = all_instances[visible_index];

    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 2u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 0.0); }
    }

    let tick = instance.start;
    let length = instance.length;
    let key = f32(note_key(instance.attrs));
    // 还原文档音轨索引（高 16 位 = track_idx + 1）
    let track = (note_track(instance.attrs)) - 1u;
    let lane = lane_index[track];

    let lh = u.lane_height;
    let key_h = lh / 128.0;
    let cox = u.canvas_offset.x;
    let coy = u.canvas_offset.y;

    // 横向：屏幕像素 x = 画布偏移 + start_tick*px_per_tick - 时间滚动
    let sx = cox + tick * u.zoom.x - u.scroll.x;
    let sw = max(length * u.zoom.x, 1.0);
    // 纵向：泳道内按音高（高音在上）定位，4px 厚条居中于音高单元格
    let lane_top = lane * lh - u.scroll.y + coy;
    let note_y = lane_top + (127.0 - key) * key_h + key_h * 0.5;
    let half_h = max(u.note_height * 0.5, 0.5);
    let sy = note_y - half_h;
    let sh = half_h * 2.0;

    let px = sx + local_offset.x * sw;
    let py = sy + local_offset.y * sh;

    // NDC（与覆盖层 arrangement.wgsl 完全一致：按 viewport_size 归一化）
    let ndc_x = (px / u.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / u.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = note_color(instance.attrs);
    output.uv = local_offset;
    output.screen_size = vec2<f32>(sw, sh);
    output.border_width = f32(note_border(instance.attrs));
    return output;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color.rgb;

    let half_w = in.screen_size.x * 0.5;
    let half_h = in.screen_size.y * 0.5;
    if (half_w > 0.0 && half_h > 0.0) {
        let b = in.border_width;
        let hx = 1.0 / half_w * b;
        let hy = 1.0 / half_h * b;
        let is_border = in.uv.x < hx
            || in.uv.x > 1.0 - hx
            || in.uv.y < hy
            || in.uv.y > 1.0 - hy;
        if (is_border) {
            color = color * BORDER_DARKEN_FACTOR;
        }
    }

    return vec4<f32>(color, in.color.a);
}
