// 走带音符 GPU 裁剪着色器（复用钢琴卷帘 cull.wgsl 的 workgroup 批量原子范式）
//
// 与 cull.wgsl 的区别：走带音符按泳道（lane）组织，而非钢琴卷帘的 (key, tick) 网格。
// 每个音符通过 border_width 高 16 位还原文档音轨索引 track，再查 lane_index[track]
// 得到泳道序号，据此计算屏幕 y；横向仍按 tick 范围判定。
//
// 输出：visible_instances 存「全局源索引」（u32），indirect_args.instance_count
// 原子累加可见数；render pass 用 draw_indirect 一次性提交，CPU 零参与绘制。

struct NoteInstance {
    start_length: vec2<f32>,  // [start_tick, length_tick]
    key_color: u32,           // 低8位=key, 高24位=RGB
    border_width: u32,        // 低16位=边框像素宽, 高16位=track_idx+1
};

struct Uniforms {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    lane_height: f32,
    note_height: f32,
    _pad: vec2<f32>,
};

struct CullUniform {
    instance_count: u32,
    chunk_start: u32,
    chunk_count: u32,
    _padding: u32,
};

struct DrawIndirectArgs {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
    _padding: vec4<u32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<uniform> cull_info: CullUniform;
@group(0) @binding(2) var<storage, read> all_instances: array<NoteInstance>;
// lane_index[doc_track] = 泳道序号（静音/隐藏轨被置为超大哨兵值，自动落出可见范围被裁掉）
@group(0) @binding(3) var<storage, read> lane_index: array<f32>;
@group(0) @binding(4) var<storage, read_write> visible_instances: array<u32>;
@group(0) @binding(5) var<storage, read_write> indirect_args: DrawIndirectArgs;

var<workgroup> wg_count: atomic<u32>;
var<workgroup> wg_indices: array<u32, 256>;
var<workgroup> wg_global_base: u32;
var<workgroup> wg_total: u32;

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    let MAX_X_THREADS: u32 = 65535u * 256u;
    let local_index = global_id.x + global_id.y * MAX_X_THREADS;
    let index = cull_info.chunk_start + local_index;
    let in_range = local_index < cull_info.chunk_count
                && index < cull_info.instance_count;

    var is_visible = false;
    if (in_range) {
        let instance = all_instances[local_index];

        let tick = instance.start_length.x;
        let length = instance.start_length.y;
        let key = f32(instance.key_color & 0xFFu);
        // 还原文档音轨索引（与 onion 编码一致：高 16 位 = track_idx + 1）
        let track = (instance.border_width >> 16u) - 1u;

        if (length > 0.0) {
            let lh = u.lane_height;
            let key_h = lh / 128.0;
            let cox = u.canvas_offset.x;
            let coy = u.canvas_offset.y;

            // 横向：与绘制着色器完全一致——宽度取 max(length*zoom, 1px)，
            // 保证裁剪框 ⊇ 绘制框，永不误删视口内可见音符。
            let sw = max(length * u.zoom.x, 1.0);
            let screen_min_x = cox + tick * u.zoom.x - u.scroll.x;
            let screen_max_x = screen_min_x + sw;
            if (screen_max_x >= 0.0 && screen_min_x <= u.viewport_size.x) {
                // 纵向：与绘制着色器同一公式——
                // 以音高单元格中心为基准、上下各 half_h 的矩形（half_h = max(note_height/2, 0.5)）。
                let lane = lane_index[track];
                let lane_top = lane * lh - u.scroll.y + coy;
                let note_y = lane_top + (127.0 - key) * key_h + key_h * 0.5;
                let half_h = max(u.note_height * 0.5, 0.5);
                let screen_min_y = note_y - half_h;
                let screen_max_y = note_y + half_h;
                if (screen_max_y >= 0.0 && screen_min_y <= u.viewport_size.y) {
                    is_visible = true;
                }
            }
        }
    }

    // Phase 1：workgroup 内本地计数（local atomic，无全局竞争）
    if (is_visible) {
        let slot = atomicAdd(&wg_count, 1u);
        // 输出全局源索引，render pass 绑定整份 all_instances，直接按索引回查
        wg_indices[slot] = local_index;
    }
    workgroupBarrier();

    // Phase 2：线程 0 做 1 次全局 atomicAdd，代表整个 workgroup
    if (local_id.x == 0u) {
        let n = atomicLoad(&wg_count);
        wg_total = n;
        wg_global_base = atomicAdd(&indirect_args.instance_count, n);
    }
    workgroupBarrier();

    // Phase 3：写入可见实例索引
    if (local_id.x < wg_total) {
        let src_idx = wg_indices[local_id.x];
        let dst = wg_global_base + local_id.x;
        if (dst < arrayLength(&visible_instances)) {
            visible_instances[dst] = src_idx;
        }
    }
}
