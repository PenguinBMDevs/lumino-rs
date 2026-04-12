// 无限网格着色器

// Viewport & Camera Uniforms
struct CameraUniform {
    viewport_size: vec2<f32>,
    camera_pos: vec2<f32>, // (scroll_x, scroll_y)
    zoom: vec2<f32>,       // (zoom_x, zoom_y)
    margins: vec2<f32>,    // (keyboard_width, ruler_height)
    color_bg: vec4<f32>,
    color_bg_black_key: vec4<f32>,
    color_bar: vec4<f32>,
    color_beat: vec4<f32>,
    color_grid: vec4<f32>,
    color_key_line: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // 画一个覆盖全屏幕的三角形/四边形 (0,0) to (1,1) 在NDC为(-1,1)到(1,-1)
    var pos = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0), // 左下
        vec2<f32>( 1.0, -1.0), // 右下
        vec2<f32>(-1.0,  1.0), // 左上
        vec2<f32>( 1.0,  1.0)  // 右上
    );

    var uv = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0)
    );

    let idx = clamp(vertex_index, 0u, 3u);

    var output: VertexOutput;
    output.position = vec4<f32>(pos[idx], 0.0, 1.0);
    output.uv = uv[idx];
    return output;
}

fn is_black_key(key: i32) -> bool {
    let k = key % 12;
    // 假设 0 是 C。黑键是 1(C#), 3(D#), 6(F#), 8(G#), 10(A#)
    let mk = k;
    if mk < 0 {
        let abs_k = (12 + mk) % 12;
        return abs_k == 1 || abs_k == 3 || abs_k == 6 || abs_k == 8 || abs_k == 10;
    }
    return k == 1 || k == 3 || k == 6 || k == 8 || k == 10;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 屏幕像素坐标 (左上为 0,0)
    let screen_x = input.uv.x * camera.viewport_size.x;
    let screen_y = input.uv.y * camera.viewport_size.y;

    // 排除键盘和标尺区域 (假设由其他 shader 渲染，网格只负责工作区，这里简单 clip 或返回背景)
    if screen_x < camera.margins.x || screen_y < camera.margins.y {
        discard;
    }

    // 将屏幕坐标转换为世界坐标
    // X轴: (screen_x - margin_x + scroll_x) / zoom_x = tick
    let world_tick = (screen_x - camera.margins.x + camera.camera_pos.x) / camera.zoom.x;
    
    // Y轴: (screen_y - margin_y + scroll_y) / zoom_y = key (向下为小)
    // 假设最高键的索引，原代码为 key = max_key - y / zoom_y
    let world_y = (screen_y - camera.margins.y + camera.camera_pos.y) / camera.zoom.y;
    let max_key: f32 = 127.0;
    let key_f32 = max_key - world_y;
    let key_int = i32(floor(key_f32));

    // 计算背景色 (黑白键)
    var bg_color = camera.color_bg;
    if is_black_key(key_int) {
        bg_color = camera.color_bg_black_key;
    }

    let ticks_per_measure: f32 = 1920.0;
    let ticks_per_beat: f32 = 480.0;
    let grid_gap: f32 = 120.0; // 1/16音符

    let line_width = 1.0;

    // Y轴网格线 (琴键分隔线)
    // 在屏幕空间中，每个键的高度是 camera.zoom.y
    let y_mod = (screen_y - camera.margins.y + camera.camera_pos.y) % camera.zoom.y;
    if y_mod < line_width {
        return mix(bg_color, camera.color_key_line, 0.8);
    }

    // X轴网格线
    // 检查是否靠近小节线
    let tick_mod_measure = world_tick % ticks_per_measure;
    let dist_measure_px = tick_mod_measure * camera.zoom.x;
    if dist_measure_px < line_width || (ticks_per_measure * camera.zoom.x - dist_measure_px) < line_width {
        return camera.color_bar;
    }

    // 检查是否靠近拍子线
    let tick_mod_beat = world_tick % ticks_per_beat;
    let dist_beat_px = tick_mod_beat * camera.zoom.x;
    if dist_beat_px < line_width || (ticks_per_beat * camera.zoom.x - dist_beat_px) < line_width {
        return mix(bg_color, camera.color_beat, 0.8);
    }

    // 检查是否靠近细分网格线
    let tick_mod_grid = world_tick % grid_gap;
    let dist_grid_px = tick_mod_grid * camera.zoom.x;
    if dist_grid_px < line_width || (grid_gap * camera.zoom.x - dist_grid_px) < line_width {
        // 只有当 zoom 足够大时才显示细分网格，避免太密
        if camera.zoom.x > 0.05 {
            return mix(bg_color, camera.color_grid, 0.6);
        }
    }

    return bg_color;
}
