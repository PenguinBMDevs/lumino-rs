// 洋葱皮可见性剔除 Compute Shader
// 工作组大小: 256
//
// 设计要点：
// - 不使用 workgroup 本地内存（wg_indices[256] 在桶模式下会溢出）。
// - 每个线程直接通过 `atomicAdd(&indirect_args.instance_count, 1u)` 获取全局槽位并写入。
// - 桶模式（use_key_ranges != 0）：每个 workgroup 处理一个 key，每个线程循环扫描该 key 的
//   [start, end) 可见 tick 子区间。一个 key 的可见音符数量不受 256 限制。
// - 兼容模式（use_key_ranges == 0）：在 [visible_start, visible_end) 区间内，
//   每个线程处理一个音符（最多 256 个/ workgroup）。
//
// 计数器重置由 CPU 端在 dispatch 前通过 write_buffer 完成，消除 GPU 端多 workgroup
// 并行执行时的竞态条件。

struct OnionNote {
    start_tick: u32,
    end_tick: u32,
    packed: u32,
    color_packed: u32,
};

struct OnionViewportUniform {
    tick_start: f32,
    tick_end: f32,
    pitch_min: f32,
    pitch_max: f32,
    note_count: u32,
    indices_capacity: u32,
    current_track: u32,
    use_key_ranges: u32,
    visible_start: u32,
    visible_end: u32,
};

struct OnionKeyRange {
    start: u32,
    end: u32,
};

struct DrawIndirectArgs {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
};

@group(0) @binding(0) var<uniform> viewport: OnionViewportUniform;
@group(0) @binding(1) var<storage, read> note_pool: array<OnionNote>;
@group(0) @binding(2) var<storage, read_write> instance_indices: array<u32>;
@group(0) @binding(3) var<storage, read_write> indirect_args: DrawIndirectArgs;
@group(0) @binding(4) var<storage, read> key_offsets: array<u32>;
@group(0) @binding(5) var<storage, read> key_ranges: array<OnionKeyRange>;

fn unpack_pitch(packed: u32) -> u32 {
    return packed & 0xFFu;
}

fn unpack_track_idx(packed: u32) -> u32 {
    return (packed >> 8u) & 0xFFFFu;
}

/// 主剔除函数：判断音符是否在视口内，可见则直写全局缓冲区。
/// 不使用 workgroup 本地内存，每个线程通过全局 atomicAdd 获取写入槽位。
fn cull_and_write(index: u32) {
    let note = note_pool[index];
    let pitch = unpack_pitch(note.packed);
    let track_idx = unpack_track_idx(note.packed);

    // 排除当前编辑音轨
    if (track_idx == viewport.current_track) {
        return;
    }

    let tick_start_f = f32(note.start_tick);
    let tick_end_f = f32(note.end_tick);
    let in_tick_range = tick_end_f > viewport.tick_start && tick_start_f < viewport.tick_end;

    let pitch_f = f32(pitch);
    let in_pitch_range = pitch_f >= viewport.pitch_min && pitch_f <= viewport.pitch_max;

    if (in_tick_range && in_pitch_range) {
        // 获取全局槽位并直写。indirect_args.instance_count 已在 dispatch 前由 CPU 归零。
        let slot = atomicAdd(&indirect_args.instance_count, 1u);
        if (slot < viewport.indices_capacity) {
            instance_indices[slot] = index;
        }
    }
}

@compute @workgroup_size(256)
fn main(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(global_invocation_id) global_id: vec3<u32>,
) {
    // 注意：indirect_args.vertex_count 和 instance_count 已由 CPU 端在 dispatch 前
    // 通过 write_buffer 初始化（vertex_count=4, instance_count=0）。
    // 无需在 GPU 端做任何重置，消除多 workgroup 并行竞态。

    if (viewport.use_key_ranges != 0u) {
        // ── Bucket 模式：一个 workgroup 处理一个 key ──
        // 每个 key 的可见音符数可能远超 256，每个线程通过 while 循环处理多个音符，
        // 每个音符直接写入全局缓冲区，不受 workgroup 本地内存限制。
        let key = workgroup_id.x;
        // key 在 [0, 256) 内；dispatch 为 256 且 WGSL 做了 bounds check，不会越界
        let pitch_f = f32(key);
        let key_active = pitch_f >= viewport.pitch_min && pitch_f <= viewport.pitch_max;

        if (key_active) {
            let range = key_ranges[key];
            let base = key_offsets[key];
            let count = range.end - range.start;

            // workgroup 内所有线程循环扫描该 key 的可见 tick 范围
            // 每步 stride=256，不共享任何 workgroup 状态，无 barrier 依赖
            var i = local_id.x;
            while (i < count) {
                cull_and_write(base + range.start + i);
                i += 256u;
            }
        }
    } else {
        // ── 兼容模式：扁平扫描 [visible_start, visible_end) ──
        // 每个 workgroup 最多处理 256 个音符（每个线程 1 个），直写全局缓冲区
        let index = viewport.visible_start + global_id.x;
        if (index < viewport.visible_end) {
            cull_and_write(index);
        }
    }
}
