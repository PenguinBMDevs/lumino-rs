// 工程走带着色器 - 实例化渲染版本
// 所有实例使用屏幕空间像素坐标，CPU 端负责坐标计算

const BORDER_DARKEN_FACTOR: f32 = 0.4;

struct Uniforms {
    viewport_size: vec2<f32>,
    scroll: vec2<f32>,
    zoom: f32,
    track_height: f32,
    notes_per_track: f32,
    zoom_y: f32,
    canvas_offset: vec2<f32>,
    playhead_x: f32,
    _pad1: f32,
    bg_color: vec4<f32>,
    bar_color: vec4<f32>,
    playhead_color: vec4<f32>,
    track_colors: array<vec4<f32>, 16>,
    track_count: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
}

struct ArrangementNoteInstance {
    @location(0) xywh: vec4<f32>,
    @location(1) packed: vec4<u32>,
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

    var x: f32 = instance.xywh.x;
    var y: f32 = instance.xywh.y;
    var w: f32 = instance.xywh.z;
    var h: f32 = instance.xywh.w;

    // 音符实例以 note-space 存储 (x=start_tick, y=key, z=length_ticks, w=lane_index)，
    // 视口变换放到着色器里完成，使 CPU 仅在音符数据变化时才重建实例，
    // 滚动/缩放只更新 uniform（消除每帧 ~67ms 的实例重建）。
    if (instance.packed.w == 3u) {
        let ppu = u.zoom;
        let lh = u.track_height * u.zoom_y;
        let key_h = lh / 128.0;
        let cox = u.canvas_offset.x;
        let coy = u.canvas_offset.y;
        let ts = u.scroll.x / ppu;
        let te = (u.scroll.x + u.viewport_size.x) / ppu;
        let left_tick = max(instance.xywh.x, ts);
        let right_tick = min(instance.xywh.x + instance.xywh.z, te);
        let sx = cox + left_tick * ppu - u.scroll.x;
        let right_screen = cox + right_tick * ppu - u.scroll.x;
        x = sx;
        w = max(right_screen - sx, 2.0);
        let lane_top = instance.xywh.w * lh - u.scroll.y + coy;
        y = lane_top + (127.0 - instance.xywh.y) * key_h;
        h = 4.0;
    }

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

    let rgba = instance.packed.x;
    out.color.r = f32((rgba >> 0u) & 0xFFu) / 255.0;
    out.color.g = f32((rgba >> 8u) & 0xFFu) / 255.0;
    out.color.b = f32((rgba >> 16u) & 0xFFu) / 255.0;
    out.color.a = f32((rgba >> 24u) & 0xFFu) / 255.0;

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
