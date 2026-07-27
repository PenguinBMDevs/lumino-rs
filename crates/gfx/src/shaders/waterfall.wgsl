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
@group(0) @binding(2) var<storage, read> active_key_colors: array<u32>;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

// ── 工具函数 ──

fn is_black_key(key: u32) -> bool {
    let k = key % 12;
    return k == 1 || k == 3 || k == 6 || k == 8 || k == 10;
}

fn unpack_color(packed: u32) -> vec4<u32> {
    let b = packed & 0xFFu;
    let g = (packed >> 8u) & 0xFFu;
    let r = (packed >> 16u) & 0xFFu;
    let a = (packed >> 24u) & 0xFFu;
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
    let note_area_h = h - min(h, kb_h);
    let ppq = params.ppq;
    let tick = params.tick;
    let speed = max(params.speed, 0.1);
    let key_count = params.key_count;

    // 计算瀑布流可见 tick 范围
    let ticks_per_measure = ppq * 4u;
    let visible_measure_count = max(u32(round(4.0 / speed)), 1u);
    let viewport_tick_span = max(ticks_per_measure * visible_measure_count, 1u);
    let tick_end = tick + viewport_tick_span;
    let zoom_x = f32(w) / f32(key_count);
    let zoom_y = f32(note_area_h) / f32(viewport_tick_span);

    // 默认黑色背景
    var pixel: u32 = pack_u32(0u, 0u, 0u, 255u);

    if y < note_area_h {
        // ── 音符区域 ──
        let note_count = arrayLength(&notes);
        for (var i = 0u; i < note_count; i++) {
            let n = notes[i];

            // 将音符的 tick 范围裁剪到视口内，避免 u32 下溢
            let visible_end = min(n.end_tick, tick_end);
            let visible_start = max(n.start_tick, tick);
            if visible_end <= visible_start {
                continue;
            }

            let note_x = u32(f32(n.key) * zoom_x);
            let note_w = u32(ceil(zoom_x));
            let note_top = u32(f32(tick_end - visible_end) * zoom_y);
            // 将 note_bottom 限制在 note_area_h 内，防止浮点精度导致音符被裁切
            let note_bottom = min(u32(f32(tick_end - visible_start) * zoom_y), note_area_h);
            let note_h = max(note_bottom - note_top, 1u);

            if x >= note_x && x < note_x + note_w && y >= note_top && y < note_top + note_h {
                let c = unpack_color(n.color_packed);
                pixel = pack_u32(c.x, c.y, c.z, 200u);
                break;
            }
        }
    } else {
        // ── 键盘区域 ──
        // 采用 Nezha 风格的两阶段渲染：
        // 阶段1：确定像素属于哪个白键
        // 阶段2：若在键盘上半部分，检查是否被黑键覆盖
        // 这样黑键下方始终露出白键，符合真实钢琴键盘外观

        let kb_y = note_area_h;
        let local_y = y - kb_y;
        let black_kb_h = u32(f32(kb_h) * 0.6);

        // 先计算白键数量
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

        // 阶段1：确定当前像素属于哪个白键
        var white_key_idx: u32 = 0u;
        var wc: u32 = 0u;
        var white_found: bool = false;
        for (var k: u32 = 0u; k < key_count && !white_found; k++) {
            if !is_black_key(k) {
                let kx_f = f32(wc) * white_w;
                let kw_f = white_w;
                if f32(x) >= kx_f && f32(x) < kx_f + kw_f {
                    white_key_idx = k;
                    white_found = true;
                }
                wc++;
            }
        }

        // 阶段2：若在键盘上半部分，检查是否在黑键区域内
        var in_black_area: bool = false;
        var black_key_idx: u32 = 0u;
        if local_y < black_kb_h {
            wc = 0u;
            for (var k: u32 = 0u; k < key_count && !in_black_area; k++) {
                if is_black_key(k) {
                    if wc > 0u {
                        let boundary_x = f32(wc) * white_w;
                        let kx_f = boundary_x - black_w_offset;
                        let kw_f = black_w;
                        if f32(x) >= kx_f && f32(x) < kx_f + kw_f {
                            black_key_idx = k;
                            in_black_area = true;
                        }
                    }
                } else {
                    wc++;
                }
            }
        }

        // 阶段3：渲染
        if white_found {
            if in_black_area {
                // 黑键覆盖区域：使用黑键颜色和黑键的活跃色
                let active_color_packed = active_key_colors[black_key_idx];
                let active_color = unpack_color(active_color_packed);
                let base = vec4<u32>(41u, 41u, 42u, 255u);
                let blended = blend_key_color(base, active_color, 153u);
                pixel = pack_u32(blended.x, blended.y, blended.z, 255u);
            } else {
                // 白键区域（含黑键下方露出的白键部分）
                let active_color_packed = active_key_colors[white_key_idx];
                let active_color = unpack_color(active_color_packed);
                let base = vec4<u32>(235u, 235u, 235u, 255u);
                let blended = blend_key_color(base, active_color, 153u);
                pixel = pack_u32(blended.x, blended.y, blended.z, 255u);
            }
        } else {
            // 未找到任何键（边界保护）：深色背景
            pixel = pack_u32(30u, 30u, 30u, 255u);
        }
    }

    textureStore(output_tex, vec2<i32>(i32(x), i32(y)), vec4<f32>(
        f32(pixel & 0xFFu) / 255.0,
        f32((pixel >> 8u) & 0xFFu) / 255.0,
        f32((pixel >> 16u) & 0xFFu) / 255.0,
        f32((pixel >> 24u) & 0xFFu) / 255.0,
    ));
}
