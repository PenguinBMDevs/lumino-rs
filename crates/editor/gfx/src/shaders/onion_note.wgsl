// 统一全量音符渲染着色器（洋葱皮 + 主音轨一体，2026-08-06）
//
// 数据模型：GPU buffer 持有**所有轨全部音符**（统一 `border_width` 高 16 位
// 编码 track_idx+1），「哪个轨是主音轨」由 `ViewState.current_track` uniform
// 低频更新（切轨零重传）。shader 内：
//   - 主音轨（track == current_track）：染主音轨蓝、深度 0（最前，覆盖一切）
//   - 静音轨且非主音轨：NDC z=2.0 裁剪（不渲染）
//   - 其余（洋葱皮）：实例固化的调色板色、深度 (track_enc+1)/65536
//
// 深度语义（修复重叠音符逐帧闪烁，2026-08）：cull.wgsl 输出可见索引，
// VS 从 all_instances 读取原数据，重叠实例的**输出顺序**由 cull 的 workgroup
// atomicAdd 抢占决定、帧间不稳定；管线是 LessEqual + depth_write_enabled=true，
// 同深度「后画者胜」→ 赢家逐帧随机。本文件用与绘制顺序无关的稳定深度消除平局，
// 完整语义见下方 tie_break_depth 前的说明块。
//
// 与旧 note.wgsl 的差异：无预览哨兵分支（预览音符走独立渲染器 note.wgsl）。
// 2026-08-07：顶点输入从完整 NoteInstance 改为 u32 可见索引。

/// 主音轨固定蓝色（与 UI 层 `MAIN_TRACK_NOTE_COLOR` 一致）
const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);

/// 边框颜色加深因子（同色系深色：color * 0.4，与主音轨 note.wgsl 保持一致）
const BORDER_DARKEN_FACTOR: f32 = 0.4;

// ── 深度语义：重叠音符逐帧闪烁修复（与 note.wgsl 保持一致的语义）──────────
//
// 根因（两半叠加）：
//   1) cull.wgsl 每个 workgroup 由线程 0 抢占式 atomicAdd 输出槽位，可见实例的
//      输出顺序由 GPU 调度决定、帧间不稳定；
//   2) 管线为 LessEqual + depth_write_enabled=true（constants.rs），同深度
//      「后画者胜」，赢家随可见缓冲顺序逐帧随机 → 重叠区描边闪烁。
// 本文件用**与绘制顺序无关**的稳定深度消除平局：
//   1) 分层：预览（note.wgsl 的 0.0，最前） < 主音轨 MAIN_TRACK_DEPTH_BASE <
//      洋葱皮轨道 (track_enc+1) × TRACK_DEPTH_STEP —— 主轨恒覆盖洋葱皮。
//   2) 轨内平局：以 chunk 内源索引 visible_index（缓冲内顺序 = 音符数据顺序，
//      跨帧稳定）派生微深度，同一轨内索引大者深度大，与绘制顺序无关。
//      洋葱皮轨道内部同样存在重叠音符，故与主音轨共用同一套裁决。
//   3) 精度预算：微深度步长 = 基深度处的一个 f32 ulp（主轨基深度处恰为 2^-40），
//      偏移上限取「到下一轨道层的 ulp 步数」与「到 NDC 远平面 z=1 的 ulp 步数」
//      的较小值的一半 —— 同轨最大偏移恒 < TRACK_DEPTH_STEP(2^-16)，不侵占相邻
//      轨道深度层，洋葱皮的轨道排序不回归；索引另受 f32 尾数可精确表示的整数
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

/// 视图状态：当前音轨（track_idx+1）+ 静音位图（512 × vec4 = 65536 轨）
struct ViewState {
    current_track: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    muted_bits: array<vec4<u32>, 512>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<uniform> view_state: ViewState;

// 全部音符实例数据（只读 storage）
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

/// 解包 key_color → vec4 RGBA（alpha 恒为 1.0）
fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

/// 查询静音位图（track_enc = track_idx+1；0 恒为未静音）
/// 布局与 CPU `ViewState` 一致：128 个连续 u32，bit i = 音轨 i 静音。
fn is_muted_track(track_enc: u32) -> bool {
    if (track_enc == 0u) {
        return false;
    }
    let track_idx = track_enc - 1u;
    let word = track_idx / 32u;
    let bit = track_idx % 32u;
    if (word >= 2048u) {
        return false;
    }
    let v = view_state.muted_bits[word / 4u];
    return ((v[word % 4u] >> bit) & 1u) != 0u;
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) visible_index: u32,
) -> VertexOutput {
    let instance = all_instances[visible_index];

    // 根据顶点索引生成矩形的四个角（三角形带顺序）
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

    // 主音轨判定：track 编码 == 当前音轨编码（track_idx+1）
    let track_enc = instance.border_width >> 16u;
    let is_main = track_enc == view_state.current_track;
    // 静音且非主音轨 → 不渲染（退化为视口外零面积几何，见下方输出分支）
    let is_muted = is_muted_track(track_enc);
    let show = is_main || !is_muted;

    // 稳定深度（与绘制顺序无关，语义见文件头）：
    //   主音轨 → MAIN_TRACK_DEPTH_BASE（最前，覆盖洋葱皮）；
    //   洋葱皮轨道 → (track_enc+1) × TRACK_DEPTH_STEP（越大越靠后）
    var track_base = f32(track_enc + 1u) * TRACK_DEPTH_STEP;
    if (is_main) {
        track_base = MAIN_TRACK_DEPTH_BASE;
    }
    // 轨内平局裁决：源索引稳定 → 同轨重叠音符胜者稳定
    let depth = tie_break_depth(track_base, visible_index);

    // 颜色：主音轨强制主轨蓝（数据无需重传）；其余用实例固化调色板色
    var color = unpack_key_color(instance.key_color);
    if (is_main) {
        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
    }

    var output: VertexOutput;
    if (show) {
        output.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    } else {
        // 静音轨：4 个顶点重合到视口外的同一点 → 零面积 → 不产生任何片元。
        // 不再依赖 NDC z=2.0 的「超出深度范围被裁剪」——那要求 depth attachment
        // 参与；退化几何与 depth attachment 是否存在无关（导出路径无 depth）。
        output.position = vec4<f32>(2.0, 2.0, 0.0, 1.0);
    }
    output.color = color;
    output.uv = local_offset;
    output.screen_size = screen_size;
    output.border_width = instance.border_width;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 纯色填充（不透明，洋葱皮 alpha=1.0 与主音轨一致）
    var color = input.color.rgb;

    // 2 像素同色加深描边（与 note.wgsl 共用 UV 边距判定算法）
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
        // 边框色 = 原色 × 0.4（同色系加深，与主音轨一致）
        color = color * BORDER_DARKEN_FACTOR;
    }

    return vec4<f32>(color, 1.0);
}
