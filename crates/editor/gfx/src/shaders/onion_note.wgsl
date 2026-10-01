// 统一全量音符渲染着色器（洋葱皮 + 主音轨一体，2026-08-06）
//
// 数据模型：GPU buffer 持有**所有轨全部音符**（统一 `border_width` 高 16 位
// 编码 track_idx+1），「哪个轨是主音轨」由 `ViewState.current_track` uniform
// 低频更新（切轨零重传）。shader 内：
//   - 主音轨（track == current_track）：染主音轨蓝、深度取主音轨区（最前，覆盖一切）
//   - 静音轨且非主音轨：视口外退化几何裁剪（不渲染）
//   - 其余（洋葱皮）：实例固化的调色板色、深度取洋葱皮区
//
// 深度语义（修复重叠音符逐帧闪烁，2026-08 / 2026-09 黑乐谱加固）：cull.wgsl
// 输出可见索引，VS 从 all_instances 读取原数据，重叠实例的**输出顺序**由 cull 的
// workgroup atomicAdd 抢占决定、帧间不稳定；管线是 LessEqual + depth_write_enabled=true，
// 同深度「后画者胜」→ 赢家逐帧随机。本文件用与绘制顺序无关的稳定深度消除平局，
// 完整语义见下方 region_depth 前的说明块。
//
// 与旧 note.wgsl 的差异：无预览哨兵分支（预览音符走独立渲染器 note.wgsl）。
// 2026-08-07：顶点输入从完整 NoteInstance 改为 u32 可见索引。

/// 主音轨固定蓝色（与 UI 层 `MAIN_TRACK_NOTE_COLOR` 一致）
const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);

/// 边框颜色加深因子（同色系深色：color * 0.4，与主音轨 note.wgsl 保持一致）
const BORDER_DARKEN_FACTOR: f32 = 0.4;

// ── 深度语义：区域化位空间映射（与 note.wgsl 保持一致的语义，2026-09 加固）──
//
// 根因（两半叠加）：
//   1) cull.wgsl 每个 workgroup 由线程 0 抢占式 atomicAdd 输出槽位，可见实例的
//      输出顺序由 GPU 调度决定、帧间不稳定；
//   2) 管线为 LessEqual + depth_write_enabled=true（constants.rs），同深度
//      「后画者胜」，赢家随可见缓冲顺序逐帧随机 → 重叠区描边闪烁。
// 本文件用**与绘制顺序无关**的稳定深度消除平局：
//   1) 分层：预览（note.wgsl 的 0.0，最前） < 主音轨区 MAIN_DEPTH_REGION_BITS <
//      洋葱皮区 ONION_DEPTH_REGION_BITS —— 主轨恒覆盖洋葱皮。
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
    //   主音轨 → 主音轨区（最前，覆盖全部洋葱皮）；
    //   洋葱皮 → 洋葱皮区（全局索引序 = 段表/轨道顺序，稳定）。
    //   全局索引 = chunk_start + 本地可见索引 → 跨 chunk 不重置、跨帧稳定。
    let region_bits = select(ONION_DEPTH_REGION_BITS, MAIN_DEPTH_REGION_BITS, is_main);
    let global_index = chunk_info.chunk_start + visible_index;
    let depth = region_depth(region_bits, global_index);

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

// ── 亚像素档位：点图元直绘入口（PREF-004 P1，2026-10-01）─────────────────────
//
// 背景（真机 RTX 2060 / 1920×1080 分段实测）：
//   缩小全景时每帧成本 = cull 1.39ms + draw 11.97ms（16M 音符全部可见）
//   ⇒ 瓶颈不是 cull 的读取带宽，而是 **图元装配 + 小三角形光栅化**
//     （每个音符 quad 只有零点几像素宽，仍要付 4 顶点 / 2 三角形的固定成本）。
//   点图元把「4 顶点 2 三角形」降为「1 顶点 1 点」，并顺带省掉整趟 cull pass
//   （可见性判定移入本入口），是这一档位唯一能显著压低 `T` 的杠杆。
//
// 与 vs_main 的差异（其余语义必须逐字一致，否则两条路径画面不一致）：
//   1) 顶点输入改为 `@builtin(instance_index)` 直接索引源缓冲——点直绘不产出
//      可见索引列表（无 cull），渲染侧因此不设顶点缓冲；
//   2) 落点取音符**长度/键高的中点**（与 quad 的视觉中心一致）；
//   3) 视口相交 / 静音轨 / 长度 ≤ 0 判定全部在本入口完成，不可见者输出视口外点；
//   4) 深度公式不变：`global_index = chunk_start + instance_index`（= 源索引），
//      与 quad 路径的 `chunk_start + 可见索引` 同值 ⇒ 两条路径深度逐位一致，
//      重叠音符的稳定裁决语义不回退；
//   5) 片元不画描边——亚像素宽度下描边没有物理意义（quad 路径在此时
//      `horiz_margin` 巨大，整块都会被判成描边色，反而更失真）。

@vertex
fn vs_point(@builtin(instance_index) instance_index: u32) -> VertexOutput {
    let instance = all_instances[instance_index];

    let tick = instance.start_length.x;
    let length = instance.start_length.y;
    let key = f32(instance.key_color & 0xFFu);

    let screen_min_x = tick * camera.zoom.x - camera.scroll.x
                       + camera.keyboard_width + camera.canvas_offset.x;
    let screen_size = vec2<f32>(length * camera.zoom.x, camera.zoom.y);
    let screen_min_y = (camera.max_key_index - key) * camera.zoom.y
                       - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;

    // 可见性判定：与 cull.wgsl 逐字同口径（长度 > 0 且与视口矩形相交）
    let in_view = length > 0.0
        && screen_min_x <= camera.viewport_size.x
        && (screen_min_x + screen_size.x) >= 0.0
        && (screen_min_y + screen_size.y) >= 0.0
        && screen_min_y <= camera.viewport_size.y;

    let track_enc = instance.border_width >> 16u;
    let is_main = track_enc == view_state.current_track;
    let is_muted = is_muted_track(track_enc);
    let show = is_main || !is_muted;

    // 稳定深度：与 vs_main 同一公式、同一 global_index 语义（见文件头深度说明）
    let region_bits = select(ONION_DEPTH_REGION_BITS, MAIN_DEPTH_REGION_BITS, is_main);
    let global_index = chunk_info.chunk_start + instance_index;
    let depth = region_depth(region_bits, global_index);

    var color = unpack_key_color(instance.key_color);
    if (is_main) {
        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
    }

    var output: VertexOutput;
    if (show && in_view) {
        let center = vec2<f32>(
            screen_min_x + screen_size.x * 0.5,
            screen_min_y + screen_size.y * 0.5,
        );
        let ndc_x = (center.x / camera.viewport_size.x) * 2.0 - 1.0;
        let ndc_y = 1.0 - (center.y / camera.viewport_size.y) * 2.0;
        output.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    } else {
        // 视口外点 → 被裁剪，不产生片元（与 vs_main 的退化几何同思路）
        output.position = vec4<f32>(2.0, 2.0, 0.0, 1.0);
    }
    output.color = color;
    output.uv = vec2<f32>(0.5, 0.5);
    output.screen_size = screen_size;
    output.border_width = instance.border_width;

    return output;
}

/// 点图元片元：纯色覆盖（不画描边，理由见 vs_point 注释第 5 条）
@fragment
fn fs_point(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color.rgb, 1.0);
}

// ── VS cull 直绘入口（PREF-005，2026-10-01）──────────────────────────────────
//
// 背景：`cull.wgsl` 的可见索引由 workgroup 间抢占式 `atomicAdd` 写入，输出顺序
// 由 GPU 调度决定 ⇒ 重叠片元按**随机深度序**到达，`LessEqual + depth_write` 下
// 无处早拒，每个片元全额付 FS + blend。实测（RTX 2060 / 1920×1080）：
//   panorama 16M 全可见：compute cull 1.60ms + draw 13.44ms
// 本入口把可见性判定搬进 VS，于是：
//   1. 不再需要 compute cull pass（省掉全量读 + 可见索引写 + indirect）；
//   2. `instance_index` 即**源索引升序** = `region_depth` 的近→远序 ⇒ 重叠片元
//      后到者被 early-Z 直接拒绝，FS 执行量降到 ≈ 屏幕像素量级；
//   3. 配合不透明管线（无 blend ROP），early-Z 不再被混合路径拖累。
//
// 与 vs_main 的差异（其余语义必须逐字一致，否则两条路径画面不一致）：
//   1) 顶点输入改为 `@builtin(instance_index)`（无顶点缓冲）；
//   2) 视口相交判定在本入口完成（谓词与 `cull.wgsl` 逐字一致），不可见/静音/
//      长度 ≤ 0 一律退化为视口外零面积点（净效果：不产生任何片元）。
//
// **硬约束**：不得在此引入 LOD 剔除（"1px 以下音符仍绘制"，见 cull.wgsl 注释）。

@vertex
fn vs_direct(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let instance = all_instances[instance_index];

    // 根据顶点索引生成矩形的四个角（三角形带顺序，与 vs_main 逐字一致）
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
    let screen_size = vec2<f32>(length * camera.zoom.x, camera.zoom.y);
    let screen_y = (camera.max_key_index - key) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;

    // 可见性：与 cull.wgsl 逐字同口径（长度 > 0 且与视口矩形相交）
    let in_view = length > 0.0
        && screen_x <= camera.viewport_size.x
        && (screen_x + screen_size.x) >= 0.0
        && (screen_y + screen_size.y) >= 0.0
        && screen_y <= camera.viewport_size.y;

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;
    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    let track_enc = instance.border_width >> 16u;
    let is_main = track_enc == view_state.current_track;
    let is_muted = is_muted_track(track_enc);
    let show = is_main || !is_muted;

    // 稳定深度：与 vs_main 同一公式。`instance_index` 与 cull 路径写入可见缓冲的
    // `local_index` 是同一个值（chunk 内源索引）⇒ 两条路径深度逐位一致。
    let region_bits = select(ONION_DEPTH_REGION_BITS, MAIN_DEPTH_REGION_BITS, is_main);
    let global_index = chunk_info.chunk_start + instance_index;
    let depth = region_depth(region_bits, global_index);

    var color = unpack_key_color(instance.key_color);
    if (is_main) {
        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
    }

    var output: VertexOutput;
    if (show && in_view) {
        output.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    } else {
        // 静音轨 / 视口外 / 非法长度：4 个顶点退化到视口外同一点 → 零面积 → 无片元。
        // 与静音轨分支同技巧，且与 depth attachment 是否存在无关。
        output.position = vec4<f32>(2.0, 2.0, 0.0, 1.0);
    }
    output.color = color;
    output.uv = local_offset;
    output.screen_size = screen_size;
    output.border_width = instance.border_width;

    return output;
}
