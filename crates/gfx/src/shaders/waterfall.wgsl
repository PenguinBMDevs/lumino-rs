// waterfall.wgsl — 瀑布流模式全 GPU 帧渲染计算着色器
//
// 每个线程处理一个像素，直接从 storage buffer 读取音符数据，
// 写入 storage texture，实现零 CPU 参与的帧渲染。
//
// dispatch: (ceil(w/16), ceil(h/16), 1)
// workgroup_size: (16, 16, 1)

// ── 数据结构 ──

struct WaterfallUniform {
    tick: u32,
    ppq: u32,
    key_count: u32,
    frame_width: u32,
    frame_height: u32,
    kb_height: u32,
    speed: f32,
    _padding: u32,
}

struct WaterfallNote {
    key: u32,
    start_tick: u32,
    end_tick: u32,
    color_packed: u32,
}

// ── Bindings ──

@group(0) @binding(0) var<uniform> params: WaterfallUniform;
@group(0) @binding(1) var<storage, read> notes: array<WaterfallNote>;
@group(0) @binding(2) var<storage, read> active_key_colors: array<u32>;  // 128 keys, packed BGRA
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

// ── 工具函数 ──

fn is_black_key(key: u32) -> bool {
    // 标准 12 键半音阶黑键判定
    let k = key % 12;
    return k == 1 || k == 3 || k == 6 || k == 8 || k == 10;
}

fn unpack_color(packed: u32) -> vec4<u32> {
    let b = (packed >> 0) & 0xFFu;
    let g = (packed >> 8) & 0xFFu;
    let r = (packed >> 16) & 0xFFu;
    let a = (packed >> 24) & 0xFFu;
    return vec4<u32>(b, g, r, a);
}

fn blend_key_color(base: vec4<u32>, overlay: vec4<u32>, alpha: u32) -> vec4<u32> {
    if overlay.a == 0u || alpha == 0u {
        return base;
    }
    let a = alpha;
    let b = (base.x * (255u - a) + overlay.x * a) / 255u;
    let g = (base.y * (255u - a) + overlay.y * a) / 255u;
    let r = (base.z * (255u - a) + overlay.z * a) / 255u;
    return vec4<u32>(b, g, r, 255u);
}

fn pack_u32(b: u32, g: u32, r: u32, a: u32) -> u32 {
    return b | (g << 8u) | (r << 16u) | (a << 24u);
}

// ── 主函数 ──

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    let w = params.frame_width;
    let h = params.frame_height;

    if x >= w || y >= h {
        return;
    }

    let kb_h = params.kb_height;
    let note_area_h = h.saturating_sub(kb_h);
    let ppq = params.ppq;
    let tick = params.tick;
    let speed = max(params.speed, 0.1);
    let key_count = params.key_count;

    // 计算瀑布流可见 tick 范围
    let ticks_per_measure = ppq * 4u;
    let visible_measure_count = max(u32(round(4.0 / speed)), 1u);
    let viewport_tick_span = max(ticks_per_measure * visible_measure_count, 1u);
    let tick_start = tick;
    let tick_end = tick + viewport_tick_span;
    let zoom_x = f32(w) / f32(key_count);
    let zoom_y = f32(note_area_h) / f32(viewport_tick_span);

    // 默认黑色背景
    var pixel: u32 = pack_u32(0u, 0u, 0u, 255u);

    if y < note_area_h {
        // ── 音符区域 ──
        // 遍历所有音符，查找覆盖此像素的
        let note_count = arrayLength(&notes);
        for (var i = 0u; i < note_count; i++) {
            let n = notes[i];
            // 计算音符在屏幕上的位置
            let note_x = u32(f32(n.key) * zoom_x);
            let note_w = u32(ceil(zoom_x));
            let note_top = u32(f32(tick_end - n.end_tick) * zoom_y);
            let note_bottom = u32(f32(tick_end - n.start_tick) * zoom_y);
            let note_h = max(note_bottom - note_top, 1u);

            // 检查像素是否在音符矩形内
            if x >= note_x && x < note_x + note_w && y >= note_top && y < note_top + note_h {
                let c = unpack_color(n.color_packed);
                pixel = pack_u32(c.x, c.y, c.z, 200u);
                break; // 最上层音符优先
            }
        }
    } else {
        // ── 键盘区域 ──
        let kb_y = note_area_h;
        let local_y = y - kb_y;
        let black_kb_h = u32(f32(kb_h) * 0.6);

        // 计算键盘布局
        var white_count: u32 = 0u;
        var found_key = false;

        for (var key: u32 = 0u; key < key_count && !found_key; key++) {
            if is_black_key(key) {
                // 黑键：位于相邻白键边界中间
                if white_count == 0u {
                    continue;
                }
                let boundary_x = f32(white_count) * (f32(w) / f32(white_count + u32(1))); // 近似
                continue;
            }
            white_count++;
        }

        // 更精确的键盘渲染：先计算白键数量
        var total_white: u32 = 0u;
        for (var k: u32 = 0u; k < key_count; k++) {
            if !is_black_key(k) {
                total_white++;
            }
        }
        if total_white == 0u {
            total_white = 1u;
        }

        let white_w = f32(w) / f32(total_white);
        let black_w = white_w * 0.65;
        let black_w_offset = black_w * 0.5;

        // 确定当前像素属于哪个键
        var kx_f: f32 = 0.0;
        var kw_f: f32 = 0.0;
        var is_black: bool = false;
        var key_idx: u32 = 0u;
        var wc: u32 = 0u;
        var found: bool = false;

        for (var k: u32 = 0u; k < key_count && !found; k++) {
            if is_black_key(k) {
                if wc == 0u {
                    continue;
                }
                let boundary_x = f32(wc) * white_w;
                kx_f = boundary_x - black_w_offset;
                kw_f = black_w;
                if f32(x) >= kx_f && f32(x) < kx_f + kw_f {
                    is_black = true;
                    key_idx = k;
                    found = true;
                }
            } else {
                kx_f = f32(wc) * white_w;
                kw_f = white_w;
                if f32(x) >= kx_f && f32(x) < kx_f + kw_f {
                    is_black = false;
                    key_idx = k;
                    found = true;
                }
                wc++;
            }
        }

        if found {
            // 获取活跃键颜色
            let active_color_packed = active_key_colors[key_idx];
            let active_color = unpack_color(active_color_packed);

            if is_black && local_y < black_kb_h {
                // 黑键区域
                let base = vec4<u32>(41u, 41u, 42u, 255u); // BGRA
                let blended = blend_key_color(base, active_color, 153u); // 60%
                pixel = pack_u32(blended.x, blended.y, blended.z, 255u);
            } else if !is_black {
                // 白键区域
                let base = vec4<u32>(235u, 235u, 235u, 255u); // BGRA
                let blended = blend_key_color(base, active_color, 153u);
                pixel = pack_u32(blended.x, blended.y, blended.z, 255u);
            }
            // 黑键区域外（底部 40%）：黑色（已被背景覆盖）
        }
    }

    textureStore(output_tex, i32(x), i32(y), vec4<f32>(
        f32(pixel & 0xFFu) / 255.0,
        f32((pixel >> 8u) & 0xFFu) / 255.0,
        f32((pixel >> 16u) & 0xFFu) / 255.0,
        f32((pixel >> 24u) & 0xFFu) / 255.0,
    ));
}
