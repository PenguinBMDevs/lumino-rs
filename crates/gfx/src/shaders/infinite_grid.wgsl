// 无限网格着色器 — 层级化 LOD 渐隐
//
// 纵向线按音乐层级组织（粗 → 细）：
//   小节线 > 拍线（4分音符） > 半拍线（8分音符） > 16分网格 > ... > 512分网格
//
// 缩放缩小时，最细的层级先淡出；落在更粗层级上的点由更粗的层级绘制。
// 小节线通过 measure_power 间隔翻倍，并在每个 power 内连续淡出。

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

const BEAT_MAX_MEASURES: f32 = 48.0;       // 拍线（4分音符）完全消失阈值
const HALF_BEAT_MAX_MEASURES: f32 = 24.0;  // 半拍线（8分音符）完全消失阈值
const MEASURE_FADE_START: f32 = 48.0;      // 小节线每个 power 的淡出起始
const MEASURE_FADE_END: f32 = 96.0;        // 小节线每个 power 的淡出结束
const MAX_MEASURE_POWER: i32 = 6;          // 小节间隔最大翻倍次数
const GRID_TIER_COUNT: i32 = 6;            // 细分网格层级数

/// 在 [max/2, max] 内从 1.0 淡出到 0.0。
fn smooth_fade(visible_measures: f32, max_measures: f32) -> f32 {
    if visible_measures >= max_measures {
        return 0.0;
    }
    let start = max_measures * 0.5;
    if visible_measures <= start {
        return 1.0;
    }
    let t = (visible_measures - start) / (max_measures - start);
    return 1.0 - t * t;
}

/// 在 [start, end] 内从 1.0 淡出到 0.0。
fn smooth_fade_range(value: f32, start: f32, end: f32) -> f32 {
    if value <= start {
        return 1.0;
    }
    if value >= end {
        return 0.0;
    }
    let t = (value - start) / (end - start);
    return 1.0 - t * t;
}

fn grid_tier_max_measures(tier: i32) -> f32 {
    return 8.0 / f32(1u << u32(tier));
}

fn grid_tier_divisor(tier: i32) -> f32 {
    return f32(4u << u32(tier));
}

fn grid_threshold_px(tier: i32) -> f32 {
    return max(0.25, 0.5 - f32(tier) * 0.05);
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

    // === 各层级 alpha ===
    let beat_alpha = smooth_fade(visible_measures, BEAT_MAX_MEASURES);
    let halfbeat_alpha = smooth_fade(visible_measures, HALF_BEAT_MAX_MEASURES);

    // === X轴坐标计算 ===
    let world_tick = (screen_x - camera.margins.x - camera.canvas_offset.x + camera.camera_pos.x) / camera.zoom.x;

    let ticks_per_beat = camera.ppq;
    let ticks_per_half_beat = camera.ppq / 2.0;

    let base_width = 1.0;
    let before_tick_zero = world_tick < 0.0;

    // X轴网格线（从粗到细检查，粗线优先绘制）
    if !before_tick_zero {
        // 小节线：从粗到细遍历所有 power，保证细密小节线淡出时更粗的小节线已可见
        for (var p: i32 = MAX_MEASURE_POWER; p >= 0; p = p - 1) {
            let power = f32(p);
            let measure_int = ticks_per_measure * pow(2.0, power);
            let fade_start = MEASURE_FADE_START * pow(2.0, power);
            let fade_end = MEASURE_FADE_END * pow(2.0, power);
            var alpha = smooth_fade_range(visible_measures, fade_start, fade_end);
            if p > 0 && visible_measures <= fade_start / 2.0 {
                alpha = 0.0;
            }
            if alpha <= 0.0 {
                continue;
            }
            let measure_width = max(2.0, 4.0 - power * 0.5);
            let measure_frac = fract(world_tick / measure_int);
            let dist_measure = min(measure_frac, 1.0 - measure_frac) * measure_int * camera.zoom.x;
            if dist_measure < measure_width * 0.5 {
                return mix(bg_color, camera.color_bar, alpha);
            }
        }

        // 拍线（4分音符）
        let beat_frac = fract(world_tick / ticks_per_beat);
        let dist_beat = min(beat_frac, 1.0 - beat_frac) * ticks_per_beat * camera.zoom.x;
        if beat_alpha > 0.0 && dist_beat < base_width * 1.5 {
            return mix(bg_color, camera.color_beat, 0.8 * beat_alpha);
        }

        // 半拍线（8分音符）
        let half_frac = fract(world_tick / ticks_per_half_beat);
        let dist_half = min(half_frac, 1.0 - half_frac) * ticks_per_half_beat * camera.zoom.x;
        if halfbeat_alpha > 0.0 && dist_half < base_width {
            return mix(bg_color, camera.color_half_beat, 0.7 * halfbeat_alpha);
        }

        // 细分网格（16分 → 512分，粗网格优先）
        for (var tier: i32 = 0; tier < GRID_TIER_COUNT; tier = tier + 1) {
            let tier_alpha = smooth_fade(visible_measures, grid_tier_max_measures(tier));
            if tier_alpha <= 0.0 {
                continue;
            }
            let interval = camera.ppq / grid_tier_divisor(tier);
            let frac = fract(world_tick / interval);
            let dist = min(frac, 1.0 - frac) * interval * camera.zoom.x;
            if dist < grid_threshold_px(tier) {
                return mix(bg_color, camera.color_grid, 0.5 * tier_alpha);
            }
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
