// 纵向卷帘无限网格着色器 — 横向卷帘完移植 + Key 范围明显分割
//
// 坐标系转置：
//   横向：X = tick * zoom_x + keyboard_width, Y = (max_key - key) * zoom_y + ruler_height
//   纵向：X = key * zoom_y,                Y = tick * zoom_x + ruler_height
//   键盘位于底部（高度 = keyboard_width），标尺位于顶部（高度 = ruler_height）
//
// 风格完移植：
//   - LOD 阈值、smooth_fade、GRID_TIERS、measure_power 翻倍淡出与横向完全一致
//   - 拍号查询、nearest_* 距离函数原样复用
//   - 颜色 uniform 同款（bar/beat/half_beat/grid/key_line/black_key bg）
//
// Key 范围明显分割：
//   - 黑白键背景仍按 is_black_key 区分
//   - 在此之上，所有键边界绘制 1px key_line
//   - 额外在 C 音（key % 12 == 0）处绘制加粗/高不透明度的八度分割线（2px，alpha 0.95），
//     使 12 键一组的八度范围肉眼可辨；128/256 边界外同样保持一致

struct CameraUniform {
    viewport_size: vec2<f32>,
    camera_pos: vec2<f32>, // (scroll_x, scroll_y)  scroll_x = tick(=Y), scroll_y = key(=X)
    zoom: vec2<f32>,       // (zoom_x, zoom_y)
    margins: vec2<f32>,    // (keyboard_height, ruler_height) 复用 GridCameraUniform.margins
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
    canvas_size: vec2<f32>,
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

fn is_c_key(key: i32) -> bool {
    // C 音：key % 12 == 0 （0=C, 12=C1 ...），八度边界
    let k = key % 12;
    if k < 0 {
        return (12 + k) % 12 == 0;
    }
    return k == 0;
}

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
    return camera.ppq * 4.0 / f32(ts.z);
}

fn ticks_per_measure(ts: vec3<u32>) -> f32 {
    return ticks_per_beat(ts) * f32(ts.y);
}

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

fn nearest_half_beat_distance(world_tick: f32, ts: vec3<u32>) -> f32 {
    let beat_dist = nearest_beat_distance(world_tick, ts);
    let tpb = ticks_per_beat(ts);
    let half = tpb * 0.5;
    return min(beat_dist, half - beat_dist);
}

fn nearest_grid_distance(world_tick: f32, interval: f32, ts: vec3<u32>) -> f32 {
    let segment_start = f32(ts.x);
    let offset = world_tick - segment_start;
    let idx = floor(offset / interval);
    let line_start = segment_start + idx * interval;
    let dist_to_start = world_tick - line_start;
    let dist_to_end = line_start + interval - world_tick;
    return min(dist_to_start, dist_to_end);
}

const BEAT_MAX_MEASURES: f32 = 48.0;
const HALF_BEAT_MAX_MEASURES: f32 = 24.0;
const MEASURE_FADE_START: f32 = 48.0;
const MEASURE_FADE_END: f32 = 96.0;
const GRID_TIER_COUNT: i32 = 6;

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
    let screen_x = input.uv.x * camera.viewport_size.x;
    let screen_y = input.uv.y * camera.viewport_size.y;

    let keyboard_h = camera.margins.x;
    // 纵向隐藏横向标尺：网格占满至画布顶部（无顶部标尺留白），仅保留底部键盘
    let grid_bottom = camera.canvas_offset.y + camera.canvas_size.y - keyboard_h;
    if screen_y > grid_bottom || screen_y < camera.canvas_offset.y {
        discard;
    }
    // X 轴无左侧键盘，铺满全宽，不做左侧 discard；但需保证在 canvas 水平范围内
    // 若 canvas 未铺满视口，左右可保留绘制（与横向保持一致的容错）

    // === X 轴：Key 坐标（横向键）===
    let local_x = screen_x - camera.canvas_offset.x + camera.camera_pos.y;
    let world_key_f = local_x / camera.zoom.y;
    // key 索引用于黑键判断与八度线：key = floor(world_key)
    let key_int = i32(floor(world_key_f));
    let in_valid_key_range = world_key_f >= 0.0 && world_key_f <= camera.max_key_index;

    var bg_color = camera.color_bg;
    if in_valid_key_range && is_black_key(key_int) {
        bg_color = camera.color_bg_black_key;
    }

    // === Y 轴可见小节数（LOD）===
    // 可用高度 = 画布高 - 底部键盘（纵向隐藏横向标尺，头部对齐键盘顶部，时间向上远离键盘）
    let pixel_height = camera.canvas_size.y - keyboard_h;
    let first_ts = get_time_signature(0.0);
    let first_tpm = ticks_per_measure(first_ts);
    let tick_height = max(pixel_height, 1.0) / camera.zoom.x;
    let visible_measures = tick_height / max(first_tpm, 1.0);

    let measure_alpha = smooth_fade_range(visible_measures, MEASURE_FADE_START, MEASURE_FADE_END);
    let beat_alpha = smooth_fade(visible_measures, BEAT_MAX_MEASURES);
    let halfbeat_alpha = smooth_fade(visible_measures, HALF_BEAT_MAX_MEASURES);

    // === Y 轴：Tick 坐标（纵向时间，头部对齐键盘顶部，向远离键盘方向递增）===
    let world_tick = (grid_bottom - screen_y + camera.camera_pos.x) / camera.zoom.x;
    let base_width = 1.0;
    let before_tick_zero = world_tick < 0.0;
    let current_ts = get_time_signature(world_tick);

    if !before_tick_zero {
        // 小节线：水平线
        if measure_alpha > 0.0 {
            let dist = nearest_measure_distance(world_tick, current_ts) * camera.zoom.x;
            if dist < 2.0 {
                return mix(bg_color, camera.color_bar, measure_alpha);
            }
        }
        // 拍线
        if beat_alpha > 0.0 {
            let dist = nearest_beat_distance(world_tick, current_ts) * camera.zoom.x;
            if dist < base_width * 1.5 {
                return mix(bg_color, camera.color_beat, 0.8 * beat_alpha);
            }
        }
        // 半拍线
        if halfbeat_alpha > 0.0 {
            let dist = nearest_half_beat_distance(world_tick, current_ts) * camera.zoom.x;
            if dist < base_width {
                return mix(bg_color, camera.color_half_beat, 0.7 * halfbeat_alpha);
            }
        }
        // 细分网格
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

    // === X 轴：键分割线（垂直线）— Key 范围明显分割 ===
    if in_valid_key_range {
        let key_frac = fract(world_key_f);
        let dist_key = min(key_frac, 1.0 - key_frac);
        // 八度边界（C 音）更醒目：阈值更大 + 更高 alpha + 略厚（通过更大阈值近似）
        let is_octave = is_c_key(key_int) || is_c_key(key_int + 1);
        if is_octave {
            if dist_key * camera.zoom.y < 1.2 {
                // 八度线使用 key_line 的更亮混合，alpha 0.95
                return mix(bg_color, camera.color_key_line, 0.95);
            }
        } else {
            if dist_key * camera.zoom.y < base_width {
                return mix(bg_color, camera.color_key_line, 0.8);
            }
        }
        // 可选：每 12 键的背景微差已由黑键实现，此处保留 C 线即可清晰分辨八度
    }

    return bg_color;
}
