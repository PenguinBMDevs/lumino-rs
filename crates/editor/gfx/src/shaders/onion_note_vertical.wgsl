// 纵向卷帘洋葱皮着色器 — onion_note.wgsl 转置版
//
// 复用 onion_note 的主音轨特别显示逻辑（ViewState.current_track 判定主轨蓝，静音轨裁剪，稳定深度），
// 仅坐标转置：
//   横向：x = tick*zoom_x - scroll_x + keyboard_width + offset.x, y = (max_key - key)*zoom_y - scroll_y + ruler + offset.y, size=(len*zoom_x, zoom_y)
//   纵向：x = key*zoom_y - scroll_y + offset.x,                 y = grid_bottom - (tick+len)*zoom_x + scroll_x,                 size=(zoom_y, len*zoom_x)
// 键盘在底部，故 X 不叠加 keyboard_width；Y 头部对齐键盘顶部（grid_bottom），向远离键盘方向递增，样式完移植。

const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);
const BORDER_DARKEN_FACTOR: f32 = 0.4;

// ── 深度语义：区域化位空间映射（与 onion_note.wgsl 逐字一致，2026-09 加固）──
//
// 根因（两半叠加）：
//   1) cull_vertical.wgsl 每个 workgroup 由线程 0 抢占式 atomicAdd 输出槽位，
//      可见实例的输出顺序由 GPU 调度决定、帧间不稳定；
//   2) 管线为 LessEqual + depth_write_enabled=true（constants.rs），同深度
//      「后画者胜」，赢家随可见缓冲顺序逐帧随机 → 重叠区描边闪烁。
// 本文件用**与绘制顺序无关**的稳定深度消除平局：
//   1) 分层：预览（note_vertical.wgsl 的 0.0，最前） < 主音轨区
//      MAIN_DEPTH_REGION_BITS < 洋葱皮区 ONION_DEPTH_REGION_BITS。
//   2) 区内全序：深度取「区域起点 + 全局源索引」的 f32 正位空间线性映射
//      （bitcast 后位模式单调 ⇒ 索引严格等价于深度序）。全局索引 =
//      chunk_start + 本地可见索引，跨 chunk 不重置；索引跨帧稳定（段内原位），
//      洋葱皮区序 = 段表/轨道顺序，与绘制顺序无关。
//   3) 容量与饱和：每区 DEPTH_REGION_SLOTS 槽（5.2 亿实例），覆盖项目
//      2.9 亿音符目标；超过槽数的极端索引饱和到区顶（确定性，但饱和段内
//      仍可能平局——超出项目目标规模）。
//   4) 值域：区域起点取最小正规格数 2^-126，洋葱区顶 0.25 < 远平面 1.0，
//      全程正规格数、无 denormal、不越远平面。

/// 主音轨区起点位模式（2^-126，最小正规格数）
const MAIN_DEPTH_REGION_BITS: u32 = 0x00800000u;
/// 洋葱皮区起点位模式 = 主区起点 + 区容量
const ONION_DEPTH_REGION_BITS: u32 = 0x1F800000u;
/// 每区槽位数（5.2 亿）：覆盖项目 2.9 亿音符目标
const DEPTH_REGION_SLOTS: u32 = 0x1F000000u;

/// 区域内深度：区域起点 + 全局源索引（超出槽数饱和到区顶）。
///
/// 正浮点位模式随数值单调递增，故同一区内索引严格等价于深度序，
/// 且值恒为正规格数、恒 < 远平面 1.0。
fn region_depth(region_bits: u32, global_index: u32) -> f32 {
    return bitcast<f32>(region_bits + min(global_index, DEPTH_REGION_SLOTS - 1u));
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

struct NoteInstance {
    start_length: vec2<f32>,
    key_color: u32,
    border_width: u32,
}
@group(0) @binding(2)
var<storage, read> all_instances: array<NoteInstance>;

// 本 chunk 的全局基准（跨 chunk 索引连续）：`chunk_start` = 本 chunk 首实例的
// 全局索引（与 cull 阶段共用同一 uniform 槽位数据，零额外分配）。
struct ChunkInfo {
    instance_count: u32,
    chunk_start: u32,
    chunk_count: u32,
    _padding: u32,
}
@group(0) @binding(3)
var<uniform> chunk_info: ChunkInfo;

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

    let grid_bottom = camera.canvas_offset.y + camera.canvas_size.y - camera.keyboard_width;
    let screen_x = key * camera.zoom.y - camera.scroll.y + camera.canvas_offset.x;
    let screen_y = grid_bottom - (tick + length) * camera.zoom.x + camera.scroll.x;
    let screen_size = vec2<f32>(camera.zoom.y, length * camera.zoom.x);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    let track_enc = instance.border_width >> 16u;
    let is_main = track_enc == view_state.current_track;
    let is_muted = is_muted_track(track_enc);
    let show = is_main || !is_muted;

    // 稳定深度（与绘制顺序无关，语义见文件头）：
    //   主音轨 → 主音轨区（最前，覆盖全部洋葱皮）；
    //   洋葱皮 → 洋葱皮区（全局索引序 = 段表/轨道顺序，稳定）。
    //   全局索引 = chunk_start + 本地可见索引 → 跨 chunk 不重置、跨帧稳定。
    let region_bits = select(ONION_DEPTH_REGION_BITS, MAIN_DEPTH_REGION_BITS, is_main);
    let global_index = chunk_info.chunk_start + visible_index;
    let depth = region_depth(region_bits, global_index);

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

    return vec4<f32>(color, 1.0);
}
