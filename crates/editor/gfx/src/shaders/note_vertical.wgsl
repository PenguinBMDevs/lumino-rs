// 纵向卷帘音符着色器 — 横向 note.wgsl 转置版
// 复用同一 NoteInstance 布局与 GPU 缓冲，仅绘制坐标转置：
//   横向：x = tick*zoom_x - scroll_x + keyboard_width, y = (max_key - key)*zoom_y - scroll_y + ruler, size=(len*zoom_x, zoom_y)
//   纵向：x = key*zoom_y - scroll_y           , y = tick*zoom_x - scroll_x + ruler,        size=(zoom_y, len*zoom_x)
// 键盘位于底部，故 X 不再叠加 keyboard_width；Y 仍叠加 ruler_height。
// 样式完移植：同款 unpack、描边、预览哨兵、深度编码（主轨/洋葱皮/预览）、圆角/边框打包。

const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;
const PREVIEW_ALPHA: f32 = 0.7;
const BORDER_DARKEN_FACTOR: f32 = 0.4;

// ── 深度语义：区域化位空间映射（与 note.wgsl 逐字一致，2026-09 黑乐谱加固）──
//
// 根因（两半叠加）：
//   1) cull_vertical.wgsl 每个 workgroup 由线程 0 抢占式 atomicAdd 输出槽位，
//      可见实例的输出顺序由 GPU 调度决定、帧间不稳定；
//   2) 管线为 LessEqual + depth_write_enabled=true（constants.rs），同深度
//      「后画者胜」，赢家随可见缓冲顺序逐帧随机 → 重叠区描边闪烁。
// 本文件用**与绘制顺序无关**的稳定深度消除平局：
//   1) 分层：预览 0.0（最前） < 主音轨区 MAIN_DEPTH_REGION_BITS < 洋葱皮区
//      ONION_DEPTH_REGION_BITS —— 预览恒覆盖主轨，主轨恒覆盖洋葱皮。
//   2) 区内全序：深度取「区域起点 + 全局源索引」的 f32 正位空间线性映射
//      （bitcast 后位模式单调 ⇒ 索引严格等价于深度序）。全局索引 =
//      chunk_start + 本地可见索引，跨 chunk 不重置；索引跨帧稳定（段内原位），
//      故重叠音符胜者恒为索引小者，与 cull 顺序/帧号无关。
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

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

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
    //   预览音符（哨兵）→ 0.0，恒覆盖主音轨；track==0（预览副本 / i2m 主轨）→
    //   主音轨区；其余（导出路径的洋葱皮编码）→ 洋葱皮区。
    //   全局索引 = chunk_start + 本地可见索引 → 跨 chunk 不重置、跨帧稳定。
    let track = instance.border_width >> 16u;
    let is_preview = instance.border_width == PREVIEW_BORDER_SENTINEL;
    let global_index = chunk_info.chunk_start + visible_index;
    var depth = 0.0;
    if (!is_preview) {
        let region_bits = select(ONION_DEPTH_REGION_BITS, MAIN_DEPTH_REGION_BITS, track == 0u);
        depth = region_depth(region_bits, global_index);
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
