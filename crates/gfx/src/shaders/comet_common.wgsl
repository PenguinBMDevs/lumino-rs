// comet_common.wgsl — Comet 风格 GPU 渲染共享代码
//
// 由 comet_renderer/shader.rs 在运行时与样式入口拼接成一个完整 WGSL 模块。

// ── 数据结构 ──

struct CometUniform {
    tick: u32,
    ppq: u32,
    key_count: u32,
    frame_width: u32,
    frame_height: u32,
    kb_height: u32,
    style: u32,
    speed: f32,
    param1: f32,
    param2: f32,
}

struct CometNote {
    key: u32,
    start_tick: u32,
    end_tick: u32,
    color_packed: u32,
    track_idx: u32,
    velocity: u32,
    channel: u32,
    _padding: u32,
}

// ── Bindings ──

@group(0) @binding(0) var<uniform> params: CometUniform;
@group(0) @binding(1) var<storage, read> notes: array<CometNote>;
@group(0) @binding(2) var<storage, read> active_keys: array<u32>;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

// ── 常量 ──

const PI: f32 = 3.14159265;

// ── 工具函数 ──

fn is_black_key(key: u32) -> bool {
    let k = key % 12u;
    return k == 1u || k == 3u || k == 6u || k == 8u || k == 10u;
}

fn unpack_color(packed: u32) -> vec4<f32> {
    let b = f32(packed & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let r = f32((packed >> 16u) & 0xFFu) / 255.0;
    let a = f32((packed >> 24u) & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

fn pack_u32_color(r: u32, g: u32, b: u32, a: u32) -> u32 {
    return b | (g << 8u) | (r << 16u) | (a << 24u);
}

fn store_pixel(x: u32, y: u32, color: vec4<f32>) {
    textureStore(output_tex, vec2<i32>(i32(x), i32(y)), color);
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let hh = h % 1.0;
    let c = v * s;
    let x = c * (1.0 - abs((hh * 6.0) % 2.0 - 1.0));
    let m = v - c;
    var rgb: vec3<f32>;
    if hh < 1.0 / 6.0 {
        rgb = vec3<f32>(c, x, 0.0);
    } else if hh < 2.0 / 6.0 {
        rgb = vec3<f32>(x, c, 0.0);
    } else if hh < 3.0 / 6.0 {
        rgb = vec3<f32>(0.0, c, x);
    } else if hh < 4.0 / 6.0 {
        rgb = vec3<f32>(0.0, x, c);
    } else if hh < 5.0 / 6.0 {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }
    return rgb + vec3<f32>(m);
}

// ── 键盘布局 ──

struct KeyLayout {
    white_count: u32,
    white_w: f32,
    black_w: f32,
    black_h: u32,
}

fn compute_key_layout(key_count: u32, frame_width: u32) -> KeyLayout {
    var white_count: u32 = 0u;
    for (var k: u32 = 0u; k < key_count; k++) {
        if !is_black_key(k) {
            white_count++;
        }
    }
    if white_count == 0u {
        white_count = 1u;
    }
    let white_w = f32(frame_width) / f32(white_count);
    return KeyLayout(white_count, white_w, white_w * 0.65, 0u);
}

fn key_layout_with_height(key_count: u32, frame_width: u32, kb_height: u32) -> KeyLayout {
    var layout = compute_key_layout(key_count, frame_width);
    layout.black_h = u32(f32(kb_height) * 0.6);
    return layout;
}

fn find_white_key_index(x: f32, key_count: u32, layout: KeyLayout) -> i32 {
    var wc: u32 = 0u;
    for (var k: u32 = 0u; k < key_count; k++) {
        if !is_black_key(k) {
            let kx = f32(wc) * layout.white_w;
            if x >= kx && x < kx + layout.white_w {
                return i32(k);
            }
            wc++;
        }
    }
    return -1;
}

fn find_black_key_index(x: f32, y: u32, key_count: u32, layout: KeyLayout) -> i32 {
    if y >= layout.black_h {
        return -1;
    }
    var wc: u32 = 0u;
    for (var k: u32 = 0u; k < key_count; k++) {
        if is_black_key(k) {
            if wc > 0u {
                let boundary_x = f32(wc) * layout.white_w;
                let kx = boundary_x - layout.black_w * 0.5;
                if x >= kx && x < kx + layout.black_w {
                    return i32(k);
                }
            }
        } else {
            wc++;
        }
    }
    return -1;
}

// ── 音符遍历辅助 ──

fn note_is_active_at(n: CometNote, tick: u32) -> bool {
    return n.start_tick <= tick && n.end_tick > tick;
}

fn note_is_visible(n: CometNote, tick_start: u32, tick_end: u32) -> bool {
    return n.end_tick > tick_start && n.start_tick < tick_end;
}
