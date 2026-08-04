// 音符渲染着色器 — 16 bytes NoteInstance（严格对齐 wasabi NoteVertex）
// 字段布局：start_length[2] + key_color + border_width（与 wasabi 一致）
// 单位差异：start/length 保留 tick（lumino 是 DAW 编辑器），wasabi 用秒
//
// VS 用 instancing + 4 顶点 quad 复刻 wasabi 的 GS「点扩展为 quad」逻辑
// （wgpu 不支持 Geometry Shader，这是 D3=A 决策的等价实现）
//
// FS 边框算法完全照搬 wasabi notes.frag：
//   - UV 边距判定（替换原像素距离判定）
//   - 边框色 = 原色 × 0.2（5 倍变暗，与 wasabi 一致）
//   - 水平方向 cos(pi*0.5*uv.x) 渐变
//   - SRGB 平方 gamma

const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;
const PREVIEW_ALPHA: f32 = 0.7;

/// 边框颜色加深因子（与 wasabi notes.frag:35 一致：color * 0.2）
const BORDER_DARKEN_FACTOR: f32 = 0.2;

struct CameraUniform {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,           // [0,1]² UV，复刻 wasabi frag_tex_coord
    @location(2) screen_size: vec2<f32>,  // 屏幕像素宽高（用于 UV 边距反算）
    @location(3) border_width: u32,       // 透传到 FS
};

// 实例数据（16 bytes，与 wasabi NoteVertex 字段对齐）
struct NoteInstance {
    @location(0) start_length: vec2<f32>,  // [start_tick, length_tick]
    @location(1) key_color: u32,           // 低8位=key, 高24位=RGB
    @location(2) border_width: u32,        // 边框像素宽（PREVIEW_BORDER_SENTINEL=预览）
};

/// 解包 key_color → vec4 RGBA（alpha 恒为 1.0，与 wasabi 一致）
fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: NoteInstance,
) -> VertexOutput {
    // 根据顶点索引生成矩形的四个角（三角形带顺序）
    // local_offset 同时作为 UV 使用（复刻 wasabi frag_tex_coord）
    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 2u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 0.0); }
    }

    // 将逻辑坐标 (tick, key) 转换为屏幕像素坐标
    // start_length.x = start_tick, start_length.y = length_tick
    let tick = instance.start_length.x;
    let length = instance.start_length.y;
    // key 从 key_color 低 8 位解码（与 wasabi 一致）
    let key = f32(instance.key_color & 0xFFu);

    let screen_x = tick * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - key) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    // size_y 固定为 1.0，通过 zoom_y 展开
    let screen_size = vec2<f32>(length * camera.zoom.x, camera.zoom.y);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    // 转换为 NDC
    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = unpack_key_color(instance.key_color);
    output.uv = local_offset;
    output.screen_size = screen_size;
    output.border_width = instance.border_width;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 预览音符：border_width 哨兵值检测，70% alpha，不画边框
    if (input.border_width == PREVIEW_BORDER_SENTINEL) {
        return vec4<f32>(input.color.rgb, input.color.a * PREVIEW_ALPHA);
    }

    // 非预览音符：照搬 wasabi notes.frag 边框算法
    var color = input.color.rgb;

    // 水平方向余弦渐变（wasabi notes.frag:19）
    // 中间亮、两边暗，让音符左右边缘自然过渡
    color = color * (1.0 + cos(3.14159265359 * 0.5 * input.uv.x)) * 0.5;

    // UV 边距判定（wasabi notes.frag:21-31）
    // 注意：wasabi 用 NDC note_size 反算像素半宽，lumino 直接有屏幕像素尺寸
    let half_width_pixels = input.screen_size.x * 0.5;
    let half_height_pixels = input.screen_size.y * 0.5;

    // 防止零除（小音符退化为无边框）
    var is_border = false;
    if (half_width_pixels > 0.0 && half_height_pixels > 0.0) {
        let horiz_margin = 1.0 / half_width_pixels * f32(input.border_width);
        let vert_margin = 1.0 / half_height_pixels * f32(input.border_width);
        is_border = input.uv.x < horiz_margin
                 || input.uv.x > 1.0 - horiz_margin
                 || input.uv.y < vert_margin
                 || input.uv.y > 1.0 - vert_margin;
    }

    if (is_border) {
        // 边框色 = 原色 × 0.2（wasabi notes.frag:35）
        color = color * BORDER_DARKEN_FACTOR;
    }

    // SRGB 平方 gamma（wasabi notes.frag:39）
    color = color * color;

    return vec4<f32>(color, input.color.a);
}
