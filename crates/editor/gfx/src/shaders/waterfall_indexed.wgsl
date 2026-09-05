// waterfall_indexed.wgsl — 瀑布流全局桶索引渲染计算着色器
//
// 与 waterfall.wgsl 逐行一致，唯一差异（全局桶集成）：
//   - 新增 binding 5 `sort_index`（u32 置换索引，常驻复用）；
//   - 桶内二分与回溯扫描经 `sort_index` 间接寻址：`notes[mid]` → `notes[sort_index[mid]]`。
// 输入不变式：`sort_index` 为全曲 (key, start) 有序置换（`GlobalBucketIndex` 一次构建），
// `key_offsets` 为全局桶边界（257 项）。窗口裁剪逻辑（tick/tick_end 二分上界、
// SEARCH_BUFFER 回溯、可见区间裁剪）与 legacy 路径完全一致，保证像素等价
// （并列 tiebreak 差异见全局桶文档，由等价 harness 量化验收）。
//
// dispatch: (ceil(w/16), ceil(h/16), 1)
// workgroup_size: (16, 16, 1)

// 桶内二分后向前回溯的最大音符数（与 waterfall.wgsl 一致）。
const SEARCH_BUFFER: u32 = 128u;

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
    start_length: vec2<f32>,  // [start_tick, length_tick]（与 NoteInstance 同布局）
    key_color: u32,           // 低8位=key，高24位=RGB
    border_width: u32,        // 本 shader 忽略（钢琴卷帘矩形管线使用）
}

// ── Bindings ──

@group(0) @binding(0) var<uniform> params: WaterfallUniform;
@group(0) @binding(1) var<storage, read> notes: array<WaterfallNote>;
@group(0) @binding(2) var<storage, read> active_key_colors: array<u32>;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<storage, read> key_offsets: array<u32>;
// 全局桶置换索引：有序位置 p 的源下标为 sort_index[p]（`GlobalBucketIndex` 常驻）。
@group(0) @binding(5) var<storage, read> sort_index: array<u32>;

// ── 工具函数 ──

fn is_black_key(key: u32) -> bool {
    let k = key % 12;
    return k == 1 || k == 3 || k == 6 || k == 8 || k == 10;
}

// 颜色解包与 miditrail_3d.wgsl 完全一致：`pack_color` 打包为 0xRRGGBBAA，
// 按 (r, g, b, a) 解包，保证瀑布流活跃键与 MIDITrail 使用相同的调色板颜色处理逻辑。
fn unpack_color(packed: u32) -> vec4<u32> {
    let r = (packed >> 24u) & 0xFFu;
    let g = (packed >> 16u) & 0xFFu;
    let b = (packed >> 8u) & 0xFFu;
    let a = packed & 0xFFu;
    return vec4<u32>(r, g, b, a);
}

// 权威 NoteInstance 解码（与 note.wgsl / cull.wgsl 的 key_color 语义一致）。
fn note_key(n: WaterfallNote) -> u32 {
    return n.key_color & 0xFFu;
}

fn note_start(n: WaterfallNote) -> u32 {
    return u32(max(n.start_length.x, 0.0));
}

fn note_end(n: WaterfallNote) -> u32 {
    return note_start(n) + u32(max(n.start_length.y, 1.0));
}

fn unpack_key_rgb(packed: u32) -> vec3<u32> {
    let rgb = packed >> 8u;
    return vec3<u32>((rgb >> 16u) & 0xFFu, (rgb >> 8u) & 0xFFu, rgb & 0xFFu);
}

// 通道顺序语义为 (r, g, b, a)，与 unpack_color 保持一致。
fn blend_key_color(base: vec4<u32>, overlay: vec4<u32>, alpha: u32) -> vec4<u32> {
    if overlay.a == 0u || alpha == 0u {
        return base;
    }
    let a = alpha;
    let r = (base.x * (255u - a) + overlay.x * a) / 255u;
    let g = (base.y * (255u - a) + overlay.y * a) / 255u;
    let b = (base.z * (255u - a) + overlay.z * a) / 255u;
    return vec4<u32>(r, g, b, 255u);
}

// 第一参数为 R 通道（低 8 位），对应 textureStore 输出的 R 分量。
fn pack_u32(r: u32, g: u32, b: u32, a: u32) -> u32 {
    return r | (g << 8u) | (b << 16u) | (a << 24u);
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
    // 预计算倒数，避免每像素除法（pixel_tick 换算用）
    let inv_zoom_y = 1.0 / max(zoom_y, 1e-6);

    // 默认黑色背景
    var pixel: u32 = pack_u32(0u, 0u, 0u, 255u);

    if y < note_area_h {
        // ── 音符区域 ──
        // 像素 → key 列（O(1) 分桶定位，与 waterfall.wgsl 同式）。
        let z = zoom_x;
        let k0 = min(u32(f32(x) / z), key_count - 1u);
        let col_left = u32(f32(k0) * z);
        let col_right = col_left + u32(ceil(z));
        var key = k0;
        if x >= col_right && k0 + 1u < key_count {
            key = k0 + 1u;
        }
        let bucket_start = key_offsets[key];
        let bucket_end = key_offsets[key + 1u];
        let bucket_len = bucket_end - bucket_start;

        // 像素 y 对应的 tick 位置（桶内二分定位上界用）
        let pixel_tick = tick_end - u32(f32(y) * inv_zoom_y);

        if bucket_len > 0u {
            // 桶内二分：第一个 start_tick > pixel_tick 的位置（上界）。
            // 索引间接是与 legacy 的唯一差异：比较对象为 sort_index[mid] 处的音符。
            var lo = bucket_start;
            var hi = bucket_end;
            while lo < hi {
                let mid = (lo + hi) / 2u;
                if note_start(notes[sort_index[mid]]) <= pixel_tick {
                    lo = mid + 1u;
                } else {
                    hi = mid;
                }
            }
            // 回溯扫描 [bucket_start, hi)：候选音符按 start_tick 升序，
            // 命中即 break。与原实现一致的矩形判定（含 u32 边界保护），
            // 并限制在 SEARCH_BUFFER 窗口内，防止密集段全桶回溯。
            var i = hi;
            var scanned: u32 = 0u;
            while i > bucket_start && scanned < SEARCH_BUFFER {
                i -= 1u;
                scanned += 1u;
                let n = notes[sort_index[i]];

                // 将音符的 tick 范围裁剪到视口内，避免 u32 下溢
                let visible_end = min(note_end(n), tick_end);
                let visible_start = max(note_start(n), tick);
                if visible_end <= visible_start {
                    continue;
                }

                let note_x = u32(f32(note_key(n)) * zoom_x);
                let note_w = u32(ceil(zoom_x));
                let note_top = u32(f32(tick_end - visible_end) * zoom_y);
                // 将 note_bottom 限制在 note_area_h 内，防止浮点精度导致音符被裁切
                let note_bottom = min(u32(f32(tick_end - visible_start) * zoom_y), note_area_h);
                let note_h = max(note_bottom - note_top, 1u);

                if x >= note_x && x < note_x + note_w && y >= note_top && y < note_top + note_h {
                    let c = unpack_key_rgb(n.key_color);
                    pixel = pack_u32(c.x, c.y, c.z, 200u);
                    break;
                }
            }
        }
    } else {
        // ── 键盘区域（与 waterfall.wgsl 逐行一致，不读音符）──
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
