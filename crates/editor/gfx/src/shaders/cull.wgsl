// 音符 GPU 裁剪着色器 — 紧凑 NoteInstance 布局 (16 bytes，与 wasabi NoteVertex 对齐)
// workgroup 批量原子版本：每 workgroup 只做 1 次全局 atomicAdd

struct NoteInstance {
    start_length: vec2<f32>,  // [start_tick, length_tick]
    key_color: u32,           // 低8位=key, 高24位=RGB
    border_width: u32,        // 边框像素宽
};

struct CameraUniform {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: f32,
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

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> cull_info: CullUniform;
@group(0) @binding(2) var<storage, read> all_instances: array<NoteInstance>;
// 2026-08-07：可见缓冲不再重包 NoteInstance，只输出 source index（u32），显存占用降为 1/4。
@group(0) @binding(3) var<storage, read_write> visible_instances: array<u32>;
@group(0) @binding(4) var<storage, read_write> indirect_args: DrawIndirectArgs;

// workgroup 共享内存：批量原子操作的临时存储
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
    // 本 chunk 内的线性 id → 全局实例索引（chunk_start 偏移）
    let local_index = global_id.x + global_id.y * MAX_X_THREADS;
    let index = cull_info.chunk_start + local_index;
    let in_range = local_index < cull_info.chunk_count
                && index < cull_info.instance_count;

    // 可见性判定（不提前 return，所有线程必须到达 barrier）
    var is_visible = false;
    if (in_range) {
        // source buffer 已按 chunk 切片绑定，用 local_index 读取
        let instance = all_instances[local_index];

        let tick = instance.start_length.x;
        let length = instance.start_length.y;
        // key 从 key_color 低 8 位解码（与 wasabi/note.wgsl 一致）
        let key = f32(instance.key_color & 0xFFu);

        if (length > 0.0) {
            let screen_min_x = tick * camera.zoom.x - camera.scroll.x
                               + camera.keyboard_width + camera.canvas_offset.x;

            if (screen_min_x <= camera.viewport_size.x) {
                let screen_max_x = screen_min_x + length * camera.zoom.x;

                if (screen_max_x >= 0.0) {
                    // 用户硬约束：删除 LOD 剔除（screen_width >= 1.0）——
                    // 1像素以下音符仍需绘制，避免视觉缺失。
                    let screen_min_y = (camera.max_key_index - key) * camera.zoom.y
                                       - camera.scroll.y
                                       + camera.ruler_height + camera.canvas_offset.y;
                    let screen_max_y = screen_min_y + camera.zoom.y;

                    if (screen_max_y >= 0.0 && screen_min_y <= camera.viewport_size.y) {
                        is_visible = true;
                    }
                }
            }
        }
    }

    // Phase 1：workgroup 内本地计数（local atomic，无全局竞争）
    if (is_visible) {
        let slot = atomicAdd(&wg_count, 1u);
        // 输出 local_index（chunk 内源索引），render pass 绑定同 chunk 的 source 切片
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

    // Phase 3：写入可见实例索引（render pass 用索引从 all_instances 读取原数据）
    if (local_id.x < wg_total) {
        let src_idx = wg_indices[local_id.x];
        let dst = wg_global_base + local_id.x;
        if (dst < arrayLength(&visible_instances)) {
            visible_instances[dst] = src_idx;
        }
    }
}
