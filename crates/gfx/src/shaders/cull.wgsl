struct NoteInstance {
    position: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
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

// 预计算视口边界，避免每个线程重复计算
fn get_viewport_bounds() -> vec4<f32> {
    return vec4<f32>(
        0.0,                                    // min_x
        0.0,                                    // min_y
        camera.viewport_size.x,               // max_x
        camera.viewport_size.y                // max_y
    );
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Rust 侧 dispatch 拆成 2D 以适配 65535 上限，每组 X 方向最多 65535 个 workgroup
    let MAX_X_THREADS: u32 = 65535u * 64u;
    let index = global_id.x + global_id.y * MAX_X_THREADS;
    if (index >= cull_info.instance_count) {
        return;
    }

    let instance = all_instances[index];

    // 将逻辑坐标 (tick, key) 变换为屏幕像素 AABB
    let tick = instance.position.x;
    let key = instance.position.y;
    let length = instance.size.x;

    // Early exit: 如果音符长度为0，直接跳过
    if (length <= 0.0) {
        return;
    }

    let screen_min_x = tick * camera.zoom.x - camera.scroll.x
                       + camera.keyboard_width + camera.canvas_offset.x;
    
    // Early exit: 如果音符在视口左侧很远，跳过
    if (screen_min_x > camera.viewport_size.x) {
        return;
    }
    
    let screen_max_x = screen_min_x + length * camera.zoom.x;
    
    // Early exit: 如果音符在视口右侧很远，跳过
    if (screen_max_x < 0.0) {
        return;
    }

    let screen_min_y = (camera.max_key_index - key) * camera.zoom.y - camera.scroll.y
                       + camera.ruler_height + camera.canvas_offset.y;
    
    // Early exit: 如果音符在视口上方或下方很远，跳过
    let screen_max_y = screen_min_y + camera.zoom.y;
    if (screen_max_y < 0.0 || screen_min_y > camera.viewport_size.y) {
        return;
    }

    // 最终可见性测试
    if (screen_max_x >= 0.0 && screen_min_x <= camera.viewport_size.x
        && screen_max_y >= 0.0 && screen_min_y <= camera.viewport_size.y) {
        let visible_index = atomicAdd(&indirect_args.instance_count, 1u);
        visible_instances[visible_index] = instance;
    }
}
