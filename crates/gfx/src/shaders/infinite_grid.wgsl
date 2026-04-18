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
    color_half_beat: vec4<f32>,
    color_grid: vec4<f32>,
    color_key_line: vec4<f32>,
    ppq: f32,
    max_key_index: f32,
    canvas_offset: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0)
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

    // 排除键盘和标尺区域
    if screen_x < camera.margins.x || screen_y < camera.margins.y {
        discard;
    }

    // === Y轴坐标计算 - 与 note.wgsl 完全对齐 ===
    // note.wgsl: screen_y = (max_key_index - key) * zoom_y - scroll_y + ruler_height + canvas_offset_y
    // 反推: key = max_key_index - (screen_y - ruler_height - canvas_offset_y + scroll_y) / zoom_y
    let local_y = screen_y - camera.margins.y - camera.canvas_offset.y + camera.camera_pos.y;
    let world_y = local_y / camera.zoom.y;
    let key_f32 = camera.max_key_index - world_y;
    let key_int = i32(floor(key_f32));

    // 计算背景色 (黑白键)
    var bg_color = camera.color_bg;
    if is_black_key(key_int) {
        bg_color = camera.color_bg_black_key;
    }

    // === X轴坐标计算 - 与 note.wgsl 完全对齐 ===
    // note.wgsl: screen_x = tick * zoom_x - scroll_x + keyboard_width + canvas_offset_x
    // 反推: tick = (screen_x - keyboard_width - canvas_offset_x + scroll_x) / zoom_x
    let world_tick = (screen_x - camera.margins.x - camera.canvas_offset.x + camera.camera_pos.x) / camera.zoom.x;

    let ticks_per_measure = camera.ppq * 4.0;
    let ticks_per_beat = camera.ppq;
    let ticks_per_half_beat = camera.ppq / 2.0;
    let ticks_per_grid = camera.ppq / 4.0;

    let base_width = 1.0;

    // Y轴网格线 (琴键分隔线) - 检测 key_index 是否为整数
    let key_frac = fract(key_f32);
    let dist_key = min(key_frac, 1.0 - key_frac);
    if dist_key * camera.zoom.y < base_width {
        return mix(bg_color, camera.color_key_line, 0.8);
    }

    // X轴网格线 - 检测 tick 是否在网格边界上
    
    // 小节线（最粗）
    let measure_frac = fract(world_tick / ticks_per_measure);
    let dist_measure = min(measure_frac, 1.0 - measure_frac) * ticks_per_measure * camera.zoom.x;
    if dist_measure < base_width * 2.0 {
        return camera.color_bar;
    }

    // 拍线 / 1/4分割线
    let beat_frac = fract(world_tick / ticks_per_beat);
    let dist_beat = min(beat_frac, 1.0 - beat_frac) * ticks_per_beat * camera.zoom.x;
    if dist_beat < base_width * 1.5 {
        // 跳过小节线位置（避免重叠）
        if dist_measure >= base_width * 2.0 {
            return mix(bg_color, camera.color_beat, 0.8);
        }
    }

    // 1/8分割线（半拍线）
    let half_frac = fract(world_tick / ticks_per_half_beat);
    let dist_half = min(half_frac, 1.0 - half_frac) * ticks_per_half_beat * camera.zoom.x;
    if dist_half < base_width {
        if camera.zoom.x > 0.02 && dist_beat >= base_width * 1.5 {
            return mix(bg_color, camera.color_half_beat, 0.7);
        }
    }

    // 1/16细分网格线
    let grid_frac = fract(world_tick / ticks_per_grid);
    let dist_grid = min(grid_frac, 1.0 - grid_frac) * ticks_per_grid * camera.zoom.x;
    if dist_grid < base_width * 0.5 {
        if camera.zoom.x > 0.06 && dist_half >= base_width {
            return mix(bg_color, camera.color_grid, 0.5);
        }
    }

    return bg_color;
}
