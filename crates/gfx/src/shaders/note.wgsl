// 音符渲染着色器 — 紧凑 NoteInstance 布局 (24 bytes)
// CPU 传逻辑坐标 (tick, key, length)，GPU 负责变换到屏幕/NDC 空间
// size_y 固定为 1.0，通过 zoom_y 展开；color 从 u32 解包
// flags bit 0 (PREVIEW_FLAG)：启用圆角矩形 + 3px 深色边框

const PREVIEW_FLAG: u32 = 1u;
const BORDER_WIDTH_PX: f32 = 3.0;
const CORNER_RADIUS_PX: f32 = 4.0;

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
    @location(1) screen_origin: vec2<f32>,
    @location(2) screen_size: vec2<f32>,
    @location(3) flags: u32,
};

// 实例数据（逻辑坐标，24 bytes 布局）
struct NoteInstance {
    @location(0) position: vec2<f32>,  // [tick, key]
    @location(1) size_x: f32,          // length
    @location(2) color_packed: u32,    // 0xRRGGBBAA
    @location(3) flags: u32,           // bit 0: PREVIEW_FLAG
};

fn unpack_color(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 24) & 0xFFu) / 255.0;
    let g = f32((packed >> 16) & 0xFFu) / 255.0;
    let b = f32((packed >> 8) & 0xFFu) / 255.0;
    let a = f32(packed & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

// 暗化颜色（用于边框）
fn darken_color(c: vec4<f32>, factor: f32) -> vec4<f32> {
    return vec4<f32>(c.r * factor, c.g * factor, c.b * factor, c.a);
}

// 圆角矩形 SDF：返回有符号距离（负值=内部，正值=外部）
fn rounded_rect_sdf(p: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let half = size * 0.5;
    let q = vec2<f32>(abs(p.x), abs(p.y)) - half + vec2<f32>(radius, radius);
    let clamped = vec2<f32>(max(q.x, 0.0), max(q.y, 0.0));
    return length(clamped) + min(max(q.x, q.y), 0.0) - radius;
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: NoteInstance,
) -> VertexOutput {
    // 根据顶点索引生成矩形的四个角（三角形带顺序）
    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 2u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 0.0); }
    }

    // 将逻辑坐标 (tick, key) 转换为屏幕像素坐标
    let screen_x = instance.position.x * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - instance.position.y) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    // size_y 固定为 1.0，通过 zoom_y 展开
    let screen_size = vec2<f32>(instance.size_x * camera.zoom.x, camera.zoom.y);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    // 转换为 NDC
    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = unpack_color(instance.color_packed);
    output.screen_origin = vec2<f32>(screen_x, screen_y);
    output.screen_size = screen_size;
    output.flags = instance.flags;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 非预览音符：直接返回原始颜色（保持向后兼容）
    if (input.flags & PREVIEW_FLAG) == 0u {
        return input.color;
    }

    // 预览音符：圆角矩形 + 3px 深色边框
    // @builtin(position) 在片元着色器中以像素坐标表示（framebuffer 空间）
    let frag_pixel = vec2<f32>(input.position.x, input.position.y);
    // 片元相对于矩形中心的局部像素坐标
    let local_pixel = frag_pixel - input.screen_origin - input.screen_size * 0.5;

    // 圆角矩形 SDF（单位：像素）
    let dist = rounded_rect_sdf(local_pixel, input.screen_size, CORNER_RADIUS_PX);

    // 平滑抗锯齿边缘（1px 过渡）
    let alpha = 1.0 - smoothstep(0.0, 1.0, dist);

    if (alpha <= 0.0) {
        discard;
    }

    // 边框检测：距矩形边界 0~3px 范围内为边框区域
    let border_dist = dist + BORDER_WIDTH_PX;
    let border_alpha = 1.0 - smoothstep(0.0, 1.0, border_dist);

    // 边框颜色 = 主体颜色暗化至 60%
    let border_color = darken_color(input.color, 0.6);
    let body_color = input.color;

    // 混合：边框区域用边框颜色，内部用主体颜色
    let final_color = mix(body_color, border_color, border_alpha);
    return vec4<f32>(final_color.rgb, final_color.a * alpha);
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn clamp(x: f32, min_val: f32, max_val: f32) -> f32 {
    return min(max(x, min_val), max_val);
}

fn min(a: f32, b: f32) -> f32 {
    return select(b, a, a < b);
}

fn max(a: f32, b: f32) -> f32 {
    return select(b, a, a > b);
}

fn abs(x: f32) -> f32 {
    return select(-x, x, x >= 0.0);
}

fn length(v: vec2<f32>) -> f32 {
    return sqrt(v.x * v.x + v.y * v.y);
}

fn select(a: f32, b: f32, cond: bool) -> f32 {
    var result: f32;
    if (cond) {
        result = a;
    } else {
        result = b;
    }
    return result;
}
