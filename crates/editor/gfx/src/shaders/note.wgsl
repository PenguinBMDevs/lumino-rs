// 音符渲染着色器 — 16 bytes NoteInstance（严格对齐 wasabi NoteVertex）
// 字段布局：start_length[2] + key_color + border_width
//
// 渲染风格（用户要求）：
//   - 纯色填充（无 cos 渐变、无 SRGB gamma 平方）
//   - 2 像素同色加深描边（border_width 字段传 2，加深系数 0.4）
//   - 预览音符：border_width 哨兵值检测，70% alpha
//
// 深度语义（修复重叠音符逐帧闪烁，2026-08）：cull.wgsl 输出可见实例的源索引，
// VS 从 all_instances storage buffer 读取原数据；重叠实例的**输出顺序**由
// cull 的 workgroup atomicAdd 抢占决定、帧间不稳定，而管线是
// LessEqual + depth_write_enabled=true——同深度「后画者胜」，赢家逐帧随机。
// 见文件后半 tie_break_depth 的完整语义说明。
//
// 2026-08-07：顶点输入从完整 NoteInstance 改为 u32 可见索引，
// 渲染时从 group(0) binding(2) 的 storage buffer 读取原实例数据。

const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;
const PREVIEW_ALPHA: f32 = 0.7;

/// 边框颜色加深因子（同色系深色：color * 0.4，比 wasabi 0.2 略亮，视觉更协调）
const BORDER_DARKEN_FACTOR: f32 = 0.4;

// ── 深度语义：重叠音符逐帧闪烁修复 ─────────────────────────────────────────
//
// 根因（两半叠加）：
//   1) cull.wgsl 每个 workgroup 由线程 0 抢占式 atomicAdd 输出槽位，可见实例的
//      输出顺序由 GPU 调度决定、帧间不稳定；
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

// 全部音符实例数据（只读 storage，与 cull 阶段读取同一份数据）
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
    @location(1) uv: vec2<f32>,           // [0,1]² UV，用于边距判定
    @location(2) screen_size: vec2<f32>,  // 屏幕像素宽高（用于 UV 边距反算）
    @location(3) border_width: u32,       // 透传到 FS
};

/// 解包 key_color → vec4 RGBA（alpha 恒为 1.0，与 wasabi 一致）
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

    // 根据顶点索引生成矩形的四个角（三角形带顺序）
    // local_offset 同时作为 UV 使用
    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 2u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 0.0); }
    }

    // 将逻辑坐标 (tick, key) 转换为屏幕像素坐标
    let tick = instance.start_length.x;
    let length = instance.start_length.y;
    let key = f32(instance.key_color & 0xFFu);

    let screen_x = tick * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - key) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    let screen_size = vec2<f32>(length * camera.zoom.x, camera.zoom.y);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    // 转换为 NDC
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
    // 预览音符：border_width 哨兵值检测，70% alpha，不画边框
    if (input.border_width == PREVIEW_BORDER_SENTINEL) {
        return vec4<f32>(input.color.rgb, input.color.a * PREVIEW_ALPHA);
    }

    // 纯色填充（用户要求：去掉 cos 渐变和 gamma 平方）
    var color = input.color.rgb;

    // 2 像素同色加深描边（UV 边距判定算法）
    let half_width_pixels = input.screen_size.x * 0.5;
    let half_height_pixels = input.screen_size.y * 0.5;

    var is_border = false;
    if (half_width_pixels > 0.0 && half_height_pixels > 0.0) {
        // 边框宽 = 低 16 位（高 16 位被深度编码占用）
        let border_px = f32(input.border_width & 0xFFFFu);
        let horiz_margin = 1.0 / half_width_pixels * border_px;
        let vert_margin = 1.0 / half_height_pixels * border_px;
        is_border = input.uv.x < horiz_margin
                 || input.uv.x > 1.0 - horiz_margin
                 || input.uv.y < vert_margin
                 || input.uv.y > 1.0 - vert_margin;
    }

    if (is_border) {
        // 边框色 = 原色 × 0.4（同色系加深，比 wasabi 0.2 略亮）
        color = color * BORDER_DARKEN_FACTOR;
    }

    return vec4<f32>(color, input.color.a);
}
