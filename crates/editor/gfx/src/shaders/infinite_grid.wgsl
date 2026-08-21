// 无限网格着色器 — 层级化 LOD 渐隐
//
// 纵向线按音乐层级组织（粗 → 细）：
//   小节线 > 拍线（4分音符） > 半拍线（8分音符） > 16分网格 > ... > 512分网格
//
// 缩放缩小时，最细的层级先淡出；落在更粗层级上的点由更粗的层级绘制。
// 小节线通过 measure_power 间隔翻倍，并在每个 power 内连续淡出。
//
// 拍号支持：
//   从 uniform 读取拍号变化列表，按 tick 查找当前拍号段，
//   动态计算每小节/每拍 tick 数，实现 3/4、6/8 等非 4/4 拍号及中途变化。

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
    canvas_offset: vec2<f32>, // (offset_x, offset_y)
    canvas_size: vec2<f32>,   // (width, height) 纵向头部对齐键盘顶部需用
    time_signature_count: u32,
    time_signatures: array<vec4<u32>, 16>,
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

// ─── 拍号查询 ───

fn get_time_signature(tick: f32) -> vec3<u32> {
    var ts = vec3<u32>(0u, 4u, 4u);
    for (var i: u32 = 0u; i < camera.time_signature_count; i = i + 1u) {
        let entry = camera.time_signatures[i];
        let t = f32(entry.x);
        if (tick >= t) {
            ts = vec3<u32>(entry.x, entry.y, entry.z);
        } else {
            break;
        }
    }
    return ts;
}

fn ticks_per_beat(ts: vec3<u32>) -> f32 {
    // ppq 是每四分音符 tick；分母 4 -> ppq，分母 8 -> ppq/2，依此类推
    return camera.ppq * 4.0 / f32(ts.z);
}

fn ticks_per_measure(ts: vec3<u32>) -> f32 {
    return ticks_per_beat(ts) * f32(ts.y);
}

// 到最近小节线的距离（当前拍号段内）
// 优化：ts 由调用方传入，避免每像素多次调用 get_time_signature 线性扫描。
fn nearest_measure_distance(world_tick: f32, ts: vec3<u32>) -> f32 {
    let tpm = ticks_per_measure(ts);
    let segment_start = f32(ts.x);
    let offset = world_tick - segment_start;
    let measure_idx = floor(offset / tpm);
    let measure_start = segment_start + measure_idx * tpm;
    let dist_to_start = world_tick - measure_start;
    let dist_to_end = measure_start + tpm - world_tick;
    return min(dist_to_start, dist_to_end);
}

// 到最近拍线的距离（当前拍号段内）
fn nearest_beat_distance(world_tick: f32, ts: vec3<u32>) -> f32 {
    let tpb = ticks_per_beat(ts);
    let segment_start = f32(ts.x);
    let offset = world_tick - segment_start;
    let beat_idx = floor(offset / tpb);
    let beat_start = segment_start + beat_idx * tpb;
    let dist_to_start = world_tick - beat_start;
    let dist_to_end = beat_start + tpb - world_tick;
    return min(dist_to_start, dist_to_end);
}

// 到最近半拍线的距离
fn nearest_half_beat_distance(world_tick: f32, ts: vec3<u32>) -> f32 {
    let beat_dist = nearest_beat_distance(world_tick, ts);
    let tpb = ticks_per_beat(ts);
    let half = tpb * 0.5;
    return min(beat_dist, half - beat_dist);
}

// 到 interval 网格线的最近距离（用于细分网格）
fn nearest_grid_distance(world_tick: f32, interval: f32, ts: vec3<u32>) -> f32 {
    let segment_start = f32(ts.x);
    let offset = world_tick - segment_start;
    let idx = floor(offset / interval);
    let line_start = segment_start + idx * interval;
    let dist_to_start = world_tick - line_start;
    let dist_to_end = line_start + interval - world_tick;
    return min(dist_to_start, dist_to_end);
}

// ─── LOD 阈值（基于可见小节数）───

const BEAT_MAX_MEASURES: f32 = 48.0;       // 拍线（4分音符）完全消失阈值
const HALF_BEAT_MAX_MEASURES: f32 = 24.0;  // 半拍线（8分音符）完全消失阈值
const MEASURE_FADE_START: f32 = 48.0;      // 小节线淡出起始
const MEASURE_FADE_END: f32 = 96.0;        // 小节线淡出结束
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

    // === 计算视口内可见小节数（LOD 核心指标，使用首个拍号近似）===
    let pixel_width = camera.viewport_size.x - camera.margins.x;
    let first_ts = get_time_signature(0.0);
    let first_ticks_per_measure = ticks_per_measure(first_ts);
    let tick_width = pixel_width / camera.zoom.x;
    let visible_measures = tick_width / first_ticks_per_measure;

    // === 各层级 alpha ===
    let measure_alpha = smooth_fade_range(visible_measures, MEASURE_FADE_START, MEASURE_FADE_END);
    let beat_alpha = smooth_fade(visible_measures, BEAT_MAX_MEASURES);
    let halfbeat_alpha = smooth_fade(visible_measures, HALF_BEAT_MAX_MEASURES);

    // === X轴坐标计算 ===
    let world_tick = (screen_x - camera.margins.x - camera.canvas_offset.x + camera.camera_pos.x) / camera.zoom.x;

    let base_width = 1.0;
    let before_tick_zero = world_tick < 0.0;

    // 优化：每个 fragment 的 world_tick 对应唯一拍号段，预先查询一次复用，
    // 避免 nearest_*_distance 内部各自调用 get_time_signature（每像素最坏 ~10 次
    // O(time_signature_count) 线性扫描）。降到 1 次查询，显著降低 GPU 着色器负担。
    let current_ts = get_time_signature(world_tick);

    // X轴网格线（从粗到细检查，粗线优先绘制）
    if !before_tick_zero {
        // 小节线
        if measure_alpha > 0.0 {
            let measure_dist = nearest_measure_distance(world_tick, current_ts) * camera.zoom.x;
            if measure_dist < 2.0 {
                return mix(bg_color, camera.color_bar, measure_alpha);
            }
        }

        // 拍线（4分音符）
        if beat_alpha > 0.0 {
            let beat_dist = nearest_beat_distance(world_tick, current_ts) * camera.zoom.x;
            if beat_dist < base_width * 1.5 {
                return mix(bg_color, camera.color_beat, 0.8 * beat_alpha);
            }
        }

        // 半拍线（8分音符）
        if halfbeat_alpha > 0.0 {
            let half_dist = nearest_half_beat_distance(world_tick, current_ts) * camera.zoom.x;
            if half_dist < base_width {
                return mix(bg_color, camera.color_half_beat, 0.7 * halfbeat_alpha);
            }
        }

        // 细分网格（16分 → 512分，粗网格优先）
        for (var tier: i32 = 0; tier < GRID_TIER_COUNT; tier = tier + 1) {
            let tier_alpha = smooth_fade(visible_measures, grid_tier_max_measures(tier));
            if tier_alpha <= 0.0 {
                continue;
            }
            let interval = ticks_per_beat(current_ts) / grid_tier_divisor(tier);
            if interval <= 0.0 {
                continue;
            }
            let dist = nearest_grid_distance(world_tick, interval, current_ts) * camera.zoom.x;
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
