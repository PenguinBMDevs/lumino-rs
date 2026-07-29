// 音符 GPU 裁剪着色器 — 紧凑 NoteInstance 布局 (24 bytes)
// workgroup 批量原子版本：每 workgroup 只做 1 次全局 atomicAdd

struct NoteInstance {
    position: vec2<f32>,
    size_x: f32,
    color_packed: u32,
    flags: u32,
    _padding: u32,
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
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
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
@group(0) @binding(3) var<storage, read_write> visible_instances: array<NoteInstance>;
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
    let index = global_id.x + global_id.y * MAX_X_THREADS;
    let in_range = index < cull_info.instance_count && index < arrayLength(&all_instances);

    // 可见性判定（不提前 return，所有线程必须到达 barrier）
    var is_visible = false;
    if (in_range) {
        let instance = all_instances[index];

        let tick = instance.position.x;
        let key = instance.position.y;
        let length = instance.size_x;

        if (length > 0.0) {
            let screen_min_x = tick * camera.zoom.x - camera.scroll.x
                               + camera.keyboard_width + camera.canvas_offset.x;

            if (screen_min_x <= camera.viewport_size.x) {
                let screen_max_x = screen_min_x + length * camera.zoom.x;

                if (screen_max_x >= 0.0) {
                    // LOD 剔除：屏幕宽度 < 1px 的音符肉眼不可见，跳过以节省
                    // vertex shader 带宽。全览缩放时大量音符宽度不足 1px，
                    // 剔除后 vertex count 可降低 50-90%。
                    let screen_width = screen_max_x - screen_min_x;
                    if (screen_width >= 1.0) {
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
    }

    // Phase 1：workgroup 内本地计数（local atomic，无全局竞争）
    if (is_visible) {
        let slot = atomicAdd(&wg_count, 1u);
        wg_indices[slot] = index;
    }
    workgroupBarrier();

    // Phase 2：线程 0 做 1 次全局 atomicAdd，代表整个 workgroup
    if (local_id.x == 0u) {
        let n = atomicLoad(&wg_count);
        wg_total = n;
        wg_global_base = atomicAdd(&indirect_args.instance_count, n);
    }
    workgroupBarrier();

    // Phase 3：写入可见实例
    if (local_id.x < wg_total) {
        let src_idx = wg_indices[local_id.x];
        let instance = all_instances[src_idx];
        let dst = wg_global_base + local_id.x;
        if (dst < arrayLength(&visible_instances)) {
            visible_instances[dst] = instance;
        }
    }
}
