// 纵向卷帘音符着色器 — 横向 note.wgsl 转置版
// 复用同一 NoteInstance 布局与 GPU 缓冲，仅绘制坐标转置：
//   横向：x = tick*zoom_x - scroll_x + keyboard_width, y = (max_key - key)*zoom_y - scroll_y + ruler, size=(len*zoom_x, zoom_y)
//   纵向：x = key*zoom_y - scroll_y           , y = tick*zoom_x - scroll_x + ruler,        size=(zoom_y, len*zoom_x)
// 键盘位于底部，故 X 不再叠加 keyboard_width；Y 仍叠加 ruler_height。
// 样式完移植：同款 unpack、描边、预览哨兵、深度编码（主轨/洋葱皮/预览）、圆角/边框打包。

const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;
const PREVIEW_ALPHA: f32 = 0.7;
const BORDER_DARKEN_FACTOR: f32 = 0.4;

// ── 深度语义：重叠音符逐帧闪烁修复（与 note.wgsl 逐字一致）────────────────
//
// 根因（两半叠加）：
//   1) cull_vertical.wgsl 每个 workgroup 由线程 0 抢占式 atomicAdd 输出槽位，
//      可见实例的输出顺序由 GPU 调度决定、帧间不稳定；
//   2) 管线为 LessEqual + depth_write_enabled=true（constants.rs），同深度
//      「后画者胜」，赢家随可见缓冲顺序逐帧随机 → 重叠区描边闪烁。
// 本文件用**与绘制顺序无关**的稳定深度消除平局：
//   1) 分层：预览 0.0（最前） < 主音轨 MAIN_TRACK_DEPTH_BASE < 洋葱皮轨道
//      (track_enc+1) × TRACK_DEPTH_STEP —— 预览恒覆盖主轨，主轨恒覆盖洋葱皮。
//   2) 轨内平局：以 chunk 内源索引 visible_index（缓冲内顺序 = 音符数据顺序，
//      跨帧稳定）派生微深度，同一轨内索引大者深度大，与绘制顺序无关。
//   3) 精度预算：微深度步长 = 基深度处的一个 f32 ulp（主轨基深度处恰为 2^-40），
//      偏移上限取「到下一轨道层的 ulp 步数」与「到 NDC 远平面 z=1 的 ulp 步数」
//      的较小值的一半 —— 同轨最大偏移恒 < TRACK_DEPTH_STEP(2^-16)，不侵占相邻
//      轨道深度层，也不会越过远平面被裁剪；索引另受 f32 尾数可精确表示的整数
//      上界 2^23 约束。

/// 相邻轨道深度间隔（2^-16）：洋葱皮轨道层的深度步长
const TRACK_DEPTH_STEP: f32 = 1.0 / 65536.0;
/// 主音轨基深度（2^-17）：最小正深度，预览层（0.0）恒覆盖主轨
const MAIN_TRACK_DEPTH_BASE: f32 = 1.0 / 131072.0;
/// 轨内微深度可用索引上界（2^23 - 1）：f32 尾数可精确表示的整数上界
const TIE_BREAK_INDEX_LIMIT: u32 = 8388607u;

/// 轨内平局裁决：把稳定的 chunk 内源索引注入基深度的尾数低位。
///
/// 正浮点位模式随数值单调递增，故 `bitcast<f32>(bits(base) + k)` 在 k 不越层时
/// 严格递增且可精确表示（等价于 `base + k × ulp(base)`，但不引入乘加舍入）。
fn tie_break_depth(base: f32, visible_index: u32) -> f32 {
    // 基深度到下一轨道层之间的可表示浮点数（ulp 步数）
    let layer_gap = bitcast<u32>(base + TRACK_DEPTH_STEP) - bitcast<u32>(base);
    // 基深度到 NDC 远平面 z = 1.0 之间的 ulp 步数（z > 1 会被远平面裁剪）
    let room_to_far = select(bitcast<u32>(1.0) - bitcast<u32>(base), 0u, base >= 1.0);
    let budget = min(layer_gap, room_to_far);
    if (budget < 2u) {
        // 顶层轨道已贴近远平面：保持原深度，不越层、不越平面
        return base;
    }
    let k = min(visible_index, min((budget - 1u) / 2u, TIE_BREAK_INDEX_LIMIT));
    return bitcast<f32>(bitcast<u32>(base) + k);
}

struct CameraUniform {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    canvas_size: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct NoteInstance {
    start_length: vec2<f32>,
    key_color: u32,
    border_width: u32,
}
@group(0) @binding(2)
var<storage, read> all_instances: array<NoteInstance>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) screen_size: vec2<f32>,
    @location(3) border_width: u32,
};

fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) visible_index: u32,
) -> VertexOutput {
    let instance = all_instances[visible_index];

    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 2u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 0.0); }
    }

    let tick = instance.start_length.x;
    let length = instance.start_length.y;
    let key = f32(instance.key_color & 0xFFu);

    // 纵向转置头部对齐键盘顶部：X = key*zoom_y - scroll_y, Y = grid_bottom - (tick+length)*zoom_x + scroll_x
    let grid_bottom = camera.canvas_offset.y + camera.canvas_size.y - camera.keyboard_width;
    let screen_x = key * camera.zoom.y - camera.scroll.y + camera.canvas_offset.x;
    let screen_y = grid_bottom - (tick + length) * camera.zoom.x + camera.scroll.x;
    let screen_size = vec2<f32>(camera.zoom.y, length * camera.zoom.x);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    // 稳定深度（与绘制顺序无关，语义见文件头）：
    //   预览音符（哨兵）→ 0.0，恒覆盖主音轨；主音轨（高 16 位=0）→ 主轨基深度；
    //   洋葱皮轨道 i（高 16 位=i+1）→ (i+1) × TRACK_DEPTH_STEP（索引越大越靠后）
    let track = instance.border_width >> 16u;
    let is_preview = instance.border_width == PREVIEW_BORDER_SENTINEL;
    var depth = 0.0;
    if (!is_preview) {
        var track_base = f32(track + 1u) * TRACK_DEPTH_STEP;
        if (track == 0u) {
            track_base = MAIN_TRACK_DEPTH_BASE;
        }
        // 轨内平局裁决：源索引稳定 → 重叠音符胜者稳定
        depth = tie_break_depth(track_base, visible_index);
    }

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    output.color = unpack_key_color(instance.key_color);
    output.uv = local_offset;
    output.screen_size = screen_size;
    output.border_width = instance.border_width;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (input.border_width == PREVIEW_BORDER_SENTINEL) {
        return vec4<f32>(input.color.rgb, input.color.a * PREVIEW_ALPHA);
    }

    var color = input.color.rgb;

    let half_width_pixels = input.screen_size.x * 0.5;
    let half_height_pixels = input.screen_size.y * 0.5;

    var is_border = false;
    if (half_width_pixels > 0.0 && half_height_pixels > 0.0) {
        let border_px = f32(input.border_width & 0xFFFFu);
        let horiz_margin = 1.0 / half_width_pixels * border_px;
        let vert_margin = 1.0 / half_height_pixels * border_px;
        is_border = input.uv.x < horiz_margin
                 || input.uv.x > 1.0 - horiz_margin
                 || input.uv.y < vert_margin
                 || input.uv.y > 1.0 - vert_margin;
    }

    if (is_border) {
        color = color * BORDER_DARKEN_FACTOR;
    }

    return vec4<f32>(color, input.color.a);
}
