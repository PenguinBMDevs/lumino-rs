// 无限网格着色器 — LOD 平滑缩放
//
// 以视口内可见小节数为核心判断依据（与 CPU 端 bars.rs 算法一致）：
// - < 12 小节 → 显示 1/16 细分网格线
// - < 24 小节 → 显示半拍线（8分音符）
// - < 48 小节 → 显示拍线（4分音符）
// - ≥ 48 小节 → 仅显示小节线（间隔自适应翻倍）

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

// ─── LOD 阈值（基于可见小节数）───

const GRID_MAX_MEASURES: f32 = 12.0;     // 1/16细分网格线可见上限
const HALFBEAT_MAX_MEASURES: f32 = 24.0; // 半拍线（8分音符）可见上限
const BEAT_MAX_MEASURES: f32 = 48.0;     // 拍线（4分音符）可见上限

/// 线型透明度（淡入：阈值处 0.0 → 半阈值处 1.0）
fn lod_alpha(visible_measures: f32, threshold: f32) -> f32 {
    if visible_measures >= threshold {
        return 0.0;
    }
    // 在 threshold → 0 范围，alpha 从 0.0 → 1.0
    let t = 1.0 - visible_measures / threshold;
    return min(t * 2.0, 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 屏幕像素坐标 (左上为 0,0)
    let screen_x = input.uv.x * camera.viewport_size.x;
    let screen_y = input.uv.y * camera.viewport_size.y;

    // 排除键盘和标尺区域（使用 canvas 局部坐标，考虑 offset）
    let margin_test_x = screen_x - camera.canvas_offset.x;
    let margin_test_y = screen_y - camera.canvas_offset.y;
    if margin_test_x < camera.margins.x || margin_test_y < camera.margins.y {
        discard;
    }

    // === Y轴坐标计算 - 与 note.wgsl 完全对齐 ===
    let local_y = screen_y - camera.margins.y - camera.canvas_offset.y + camera.camera_pos.y;
    let world_y = local_y / camera.zoom.y;
    let key_f32 = camera.max_key_index - world_y;
    let key_int = i32(ceil(key_f32));

    // 检查是否在有效 key 范围内 [0, max_key_index]
    let in_valid_key_range = key_f32 >= 0.0 && key_f32 <= camera.max_key_index;

    // 计算背景色 (黑白键)
    var bg_color = camera.color_bg;
    if in_valid_key_range && is_black_key(key_int) {
        bg_color = camera.color_bg_black_key;
    }

    // === 计算视口内可见小节数（LOD 核心指标）===
    let pixel_width = camera.viewport_size.x - camera.margins.x;
    let ticks_per_measure = camera.ppq * 4.0;
    let tick_width = pixel_width / camera.zoom.x;
    let visible_measures = tick_width / ticks_per_measure;

    // ─── 各线型 LOD alpha ───
    let grid_alpha = lod_alpha(visible_measures, GRID_MAX_MEASURES);
    let halfbeat_alpha = lod_alpha(visible_measures, HALFBEAT_MAX_MEASURES);
    let beat_alpha = lod_alpha(visible_measures, BEAT_MAX_MEASURES);

    // === X轴坐标计算 ===
    let world_tick = (screen_x - camera.margins.x - camera.canvas_offset.x + camera.camera_pos.x) / camera.zoom.x;

    let ticks_per_beat = camera.ppq;
    let ticks_per_half_beat = camera.ppq / 2.0;
    let ticks_per_grid = camera.ppq / 4.0;

    let base_width = 1.0;
    let before_tick_zero = world_tick < 0.0;

    // 计算各线型距离（供后续 LOD 层级引用）
    let measure_frac = fract(world_tick / ticks_per_measure);
    let dist_measure = min(measure_frac, 1.0 - measure_frac) * ticks_per_measure * camera.zoom.x;

    // 拍线距离
    let beat_frac = fract(world_tick / ticks_per_beat);
    let dist_beat = min(beat_frac, 1.0 - beat_frac) * ticks_per_beat * camera.zoom.x;

    // 半拍线距离
    let half_frac = fract(world_tick / ticks_per_half_beat);
    let dist_half = min(half_frac, 1.0 - half_frac) * ticks_per_half_beat * camera.zoom.x;

    // 1/16网格线距离
    let grid_frac = fract(world_tick / ticks_per_grid);
    let dist_grid = min(grid_frac, 1.0 - grid_frac) * ticks_per_grid * camera.zoom.x;

    // X轴网格线（先检查）：纵向线
    if !before_tick_zero {
        // 小节线（始终可见，最粗）
        if dist_measure < base_width * 2.0 {
            return camera.color_bar;
        }

        // 拍线（LOD: < 48 小节可见）
        if beat_alpha > 0.0 && dist_beat < base_width * 1.5 && dist_measure >= base_width * 2.0 {
            return mix(bg_color, camera.color_beat, 0.8 * beat_alpha);
        }

        // 半拍线（LOD: < 24 小节可见）
        if halfbeat_alpha > 0.0 && dist_half < base_width
            && dist_beat >= base_width * 1.5
        {
            return mix(bg_color, camera.color_half_beat, 0.7 * halfbeat_alpha);
        }

        // 1/16细分网格线（LOD: < 12 小节可见）
        if grid_alpha > 0.0 && dist_grid < base_width * 0.5
            && dist_half >= base_width
        {
            return mix(bg_color, camera.color_grid, 0.5 * grid_alpha);
        }
    }

    // Y轴网格线（后检查）：横向琴键分隔线
    if in_valid_key_range {
        let key_frac = fract(key_f32);
        let dist_key = min(key_frac, 1.0 - key_frac);
        if dist_key * camera.zoom.y < base_width {
            return mix(bg_color, camera.color_key_line, 0.8);
        }
    }

    return bg_color;
}
