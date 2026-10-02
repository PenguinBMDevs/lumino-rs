// 音符渲染着色器 — 16 bytes NoteInstance（严格对齐 wasabi NoteVertex）
// 字段布局：start_length[2] + key_color + border_width
//
// 渲染风格（用户要求）：
//   - 纯色填充（无 cos 渐变、无 SRGB gamma 平方）
//   - 2 像素同色加深描边（border_width 字段传 2，加深系数 0.4）
//   - 预览音符：border_width 哨兵值检测，70% alpha
//
// 深度语义（修复重叠音符逐帧闪烁，2026-08 / 2026-09 黑乐谱加固）：cull.wgsl
// 输出可见实例的源索引，VS 从 all_instances storage buffer 读取原数据；重叠实例
// 的**输出顺序**由 cull 的 workgroup atomicAdd 抢占决定、帧间不稳定，而管线是
// LessEqual + depth_write_enabled=true——同深度「后画者胜」，赢家逐帧随机。
// 见文件后半 region_depth 的完整语义说明。
//
// 2026-08-07：顶点输入从完整 NoteInstance 改为 u32 可见索引，
// 渲染时从 group(0) binding(2) 的 storage buffer 读取原实例数据。

const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;
const PREVIEW_ALPHA: f32 = 0.7;

/// 边框颜色加深因子（同色系深色：color * 0.4，比 wasabi 0.2 略亮，视觉更协调）
const BORDER_DARKEN_FACTOR: f32 = 0.4;

// ── 深度语义：区域化位空间映射（确定性 z-order，2026-09 黑乐谱加固）────────
//
// 根因（两半叠加）：
//   1) cull.wgsl 每个 workgroup 由线程 0 抢占式 atomicAdd 输出槽位，可见实例的
//      输出顺序由 GPU 调度决定、帧间不稳定；
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

// 全部音符实例数据（只读 storage，与 cull 阶段读取同一份数据）
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

// ── VS 直绘入口（预览层 z-order 修复，2026-10）─────────────────────────────
//
// 根因：预览层（`renderers.note`）此前只有 cull 路径——可见实例槽位由
// `cull.wgsl` 线程 0 抢占式 `atomicAdd` 分配，**输出顺序由 GPU 调度决定、
// 逐帧随机**；而预览实例深度恒为 0.0（下方 `is_preview` 分支），两个矩形重叠时
// 「后画者胜」的赢家就随 cull 顺序逐帧翻转 → 重叠区闪烁。触发条件真实存在：
// 多笔画 / 多音轨（`brush_track_for_level` 按粗细层映射音轨）在同一格上产出
// 颜色不同的多个预览矩形（见 `brush/cells.rs::brush_preview_runs_in_window`）。
// 本入口把可见性判定搬进 VS，实例序 = **提交序**（`instance_index` 升序），
// 于是重叠区恒为「后来者居上」：后提交者后到，深度同为 0.0 时 LessEqual 通过 →
// 覆盖先到者，且**跨帧稳定**；同时不再需要 compute cull pass（预览实例已由视口
// 窗口界定，见 §18）。
//
// 与 vs_main 的差异（深度/取色语义必须逐字一致，否则两条路径画面不一致）：
//   1) 实例索引来自 `@builtin(instance_index)`（不读可见索引顶点缓冲）；
//   2) 视口相交判定在本入口完成（谓词与 `cull.wgsl` 逐字一致），不可见/长度 ≤ 0
//      一律退化为视口外零面积点（净效果：不产生任何片元）。
@vertex
fn vs_direct(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let instance = all_instances[instance_index];

    // 根据顶点索引生成矩形的四个角（与 vs_main 逐字一致）
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

    let screen_x = tick * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - key) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    let screen_size = vec2<f32>(length * camera.zoom.x, camera.zoom.y);

    // 可见性：与 cull.wgsl 逐字同口径（长度 > 0 且与视口矩形相交）
    let in_view = length > 0.0
        && screen_x <= camera.viewport_size.x
        && (screen_x + screen_size.x) >= 0.0
        && (screen_y + screen_size.y) >= 0.0
        && screen_y <= camera.viewport_size.y;

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;
    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    // 稳定深度：与 vs_main 同一公式。`instance_index` 与 cull 路径写入可见缓冲的
    // `local_index` 是同一个值（chunk 内源索引）⇒ 两条路径深度逐位一致。
    let track = instance.border_width >> 16u;
    let is_preview = instance.border_width == PREVIEW_BORDER_SENTINEL;
    let global_index = chunk_info.chunk_start + instance_index;
    var depth = 0.0;
    if (!is_preview) {
        let region_bits = select(ONION_DEPTH_REGION_BITS, MAIN_DEPTH_REGION_BITS, track == 0u);
        depth = region_depth(region_bits, global_index);
    }

    var output: VertexOutput;
    if (in_view) {
        output.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    } else {
        // 视口外 / 非法长度：4 顶点退化到视口外同一点 → 零面积 → 无片元
        output.position = vec4<f32>(2.0, 2.0, 0.0, 1.0);
    }
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
