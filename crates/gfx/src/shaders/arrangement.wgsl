// 工程走带着色器 - 实例化渲染版本 (参考 yinhe 实现)
// 所有实例使用屏幕空间像素坐标，CPU 端负责坐标变换

const BORDER_DARKEN_FACTOR: f32 = 0.4;

struct Uniforms {
    viewport_size: vec2<f32>,      // offset 0
    scroll: vec2<f32>,             // offset 8
    zoom: f32,                     // offset 16
    track_height: f32,             // offset 20
    notes_per_track: f32,          // offset 24
    _pad0: f32,                    // offset 28
    canvas_offset: vec2<f32>,      // offset 32
    playhead_x: f32,               // offset 40
    _pad1: f32,                    // offset 44
    bg_color: vec4<f32>,           // offset 48
    bar_color: vec4<f32>,          // offset 64
    playhead_color: vec4<f32>,     // offset 80
    track_colors: array<vec4<f32>, 16>, // offset 96
    track_count: f32,              // offset 352
    _pad2: f32,                    // offset 356
    _pad3: f32,                    // offset 360
    _pad4: f32,                    // offset 364
}

struct ArrangementNoteInstance {
    @location(0) xywh: vec4<f32>,      // x, y, w, h (屏幕空间像素坐标)
    @location(1) packed: vec4<u32>,    // x=rgba, y=props, z=velocity, w=tag
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) radius: f32,
    @location(4) border_width: f32,
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: ArrangementNoteInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let x = instance.xywh.x;
    let y = instance.xywh.y;
    let w = instance.xywh.z;
    let h = instance.xywh.w;

    // 6 顶点组成 2 个三角形 (TriangleList)
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(x + w, y),
        vec2<f32>(x + w, y + h),
        vec2<f32>(x,     y),
        vec2<f32>(x + w, y + h),
        vec2<f32>(x,     y + h),
        vec2<f32>(x,     y),
    );

    var uv = array<vec2<f32>, 6>(
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 0.0),
    );

    let pixel_pos = pos[vertex_index];
    let ndc_x = (pixel_pos.x / u.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / u.viewport_size.y) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);

    // Unpack RGBA from packed u32 (4x UNORM8)
    let rgba = instance.packed.x;
    out.color.r = f32((rgba >> 0u) & 0xFFu) / 255.0;
    out.color.g = f32((rgba >> 8u) & 0xFFu) / 255.0;
    out.color.b = f32((rgba >> 16u) & 0xFFu) / 255.0;
    out.color.a = f32((rgba >> 24u) & 0xFFu) / 255.0;

    // Unpack props from packed u32 (2x f16)
    let props = instance.packed.y;
    out.radius = unpack2x16float(props).x;
    out.border_width = unpack2x16float(props).y;

    out.uv = uv[vertex_index];
    out.half_size = vec2<f32>(w, h) * 0.5;
    return out;
}

// SDF rounded box
fn sd_rounded_box(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let d = abs(p) - half_size + r;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0) - r;
}

// Border + fill alpha compositing
fn composite_border_fill(fill_a: f32, border_a: f32, color: vec4<f32>) -> vec4<f32> {
    let total_a = fill_a + border_a;
    let border_color = color.rgb * BORDER_DARKEN_FACTOR;
    var rgb = border_color;
    if fill_a > 0.0 {
        rgb = (color.rgb * fill_a + border_color * border_a) / total_a;
    }
    return vec4(rgb, color.a * total_a);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv - 0.5) * in.half_size * 2.0;

    // Fast path: no rounded corners
    if in.radius < 0.5 {
        let d_outer = max(abs(p.x) - in.half_size.x, abs(p.y) - in.half_size.y);
        let outer_a = 1.0 - smoothstep(-0.5, 0.5, d_outer);

        let inner_half = max(in.half_size - vec2(in.border_width), vec2(0.0));
        var fill_a: f32 = 0.0;
        var border_a: f32 = outer_a;

        if inner_half.x > 0.0 && inner_half.y > 0.0 {
            let d_inner = max(abs(p.x) - inner_half.x, abs(p.y) - inner_half.y);
            let inner_a = 1.0 - smoothstep(-0.5, 0.5, d_inner);
            fill_a = inner_a;
            border_a = outer_a - inner_a;
        }

        return composite_border_fill(fill_a, border_a, in.color);
    }

    // Slow path: SDF rounded rectangle
    let d_outer = sd_rounded_box(p, in.half_size, in.radius);
    let outer_a = 1.0 - smoothstep(-0.5, 0.5, d_outer);

    let inner_half = max(in.half_size - vec2(in.border_width), vec2(0.0));
    let inner_r = max(in.radius - in.border_width, 0.0);

    var fill_a: f32 = 0.0;
    var border_a: f32 = outer_a;

    if inner_half.x > 0.0 && inner_half.y > 0.0 {
        let d_inner = sd_rounded_box(p, inner_half, inner_r);
        let inner_a = 1.0 - smoothstep(-0.5, 0.5, d_inner);
        fill_a = inner_a;
        border_a = outer_a - inner_a;
    }

    return composite_border_fill(fill_a, border_a, in.color);
}
