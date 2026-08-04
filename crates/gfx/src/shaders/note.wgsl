// 音符渲染着色器 - 紧凑 NoteInstance 布局 (24 bytes)
// CPU 传逻辑坐标 (tick, key, length)，GPU 负责变换到屏幕/NDC 空间
// size_y 固定为 1.0，通过 zoom_y 展开；color 从 u32 解包
// flags bit 0 (PREVIEW_FLAG)：预览音符，透明度70%的普通音符样式

const PREVIEW_FLAG: u32 = 1u;
const PREVIEW_ALPHA: f32 = 0.7;

/// 主音轨音符边框宽度（像素）
const BORDER_WIDTH: f32 = 4.0;
/// 边框颜色加深因子（RGB 乘以此值得到边框色）
const BORDER_DARKEN_FACTOR: f32 = 0.65;

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
    // 预览音符：透明度70%的普通音符样式，不加边框
    if (input.flags & PREVIEW_FLAG) != 0u {
        return vec4<f32>(input.color.rgb, input.color.a * PREVIEW_ALPHA);
    }

    // 非预览音符：绘制深色边框
    let pixel_pos = input.position.xy;
    let note_min = input.screen_origin;
    let note_max = input.screen_origin + input.screen_size;

    // 计算到最近边缘的距离
    let dist_left = pixel_pos.x - note_min.x;
    let dist_right = note_max.x - pixel_pos.x;
    let dist_top = pixel_pos.y - note_min.y;
    let dist_bottom = note_max.y - pixel_pos.y;
    let min_dist = min(min(dist_left, dist_right), min(dist_top, dist_bottom));

    // 边框宽度不超过音符最小尺寸的一半（防止小音符全变边框）
    let half_min_dim = min(input.screen_size.x, input.screen_size.y) * 0.5;
    let effective_border = min(BORDER_WIDTH, half_min_dim);

    if (min_dist < effective_border) {
        // 边框像素：使用比填充色更深的颜色
        let border_rgb = input.color.rgb * BORDER_DARKEN_FACTOR;
        return vec4<f32>(border_rgb, input.color.a);
    }

    // 内部像素：原始颜色
    return input.color;
}
