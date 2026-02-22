struct NoteInstance {
    position: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
};

struct ViewportUniform {
    size: vec2<f32>,
    _padding: vec2<f32>,
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

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;
@group(0) @binding(1) var<uniform> cull_info: CullUniform;
@group(0) @binding(2) var<storage, read> all_instances: array<NoteInstance>;
@group(0) @binding(3) var<storage, read_write> visible_instances: array<NoteInstance>;
@group(0) @binding(4) var<storage, read_write> indirect_args: DrawIndirectArgs;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    if (index >= cull_info.instance_count) {
        return;
    }

    let instance = all_instances[index];

    // 简单的 AABB 裁剪
    let min_x = instance.position.x;
    let min_y = instance.position.y;
    let max_x = min_x + instance.size.x;
    let max_y = min_y + instance.size.y;

    // 视口边界
    let vp_min_x = 0.0;
    let vp_min_y = 0.0;
    let vp_max_x = viewport.size.x;
    let vp_max_y = viewport.size.y;

    // 检查是否相交
    if (max_x >= vp_min_x && min_x <= vp_max_x && max_y >= vp_min_y && min_y <= vp_max_y) {
        // 原子增加实例数量，并获取当前索引
        let visible_index = atomicAdd(&indirect_args.instance_count, 1u);

        // 将实例数据写入可见实例缓冲区
        visible_instances[visible_index] = instance;
    }
}
