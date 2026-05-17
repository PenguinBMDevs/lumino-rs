// ─── 洋葱皮背景瓦片渲染着色器 ─────────────────────────────────
//
// 通过 uniform buffer 传入四边形变换参数（替代 push constants，
// 兼容不支持 push constants 的 GPU 后端）。
//
// 绑定组布局：
//   @group(0) @binding(0): tile_texture  — 瓦片像素纹理
//   @group(0) @binding(1): tile_sampler  — 采样器
//   @group(0) @binding(2): push          — uniform buffer（48 bytes，16 字节对齐）

// ─── Uniform Buffer ────────────────────────────────────────────
//
// 包含四边形 NDC 变换和 UV 映射参数。
// 总计 48 bytes（填充至 16 字节对齐，满足 uniform 布局要求）。
struct PushConstants {
    position: vec2<f32>,   // 四边形左下角 NDC 坐标  [offset 0]
    size: vec2<f32>,       // 四边形尺寸（NDC 空间） [offset 8]
    uv_offset: vec2<f32>,  // UV 原点                 [offset 16]
    uv_scale: vec2<f32>,   // UV 缩放                 [offset 24]
    track_index: u32,      // 音轨索引（pool index）   [offset 32]
    _pad0: u32,            // padding 至 16 字节对齐   [offset 36]
    _pad1: u32,            //                          [offset 40]
    _pad2: u32,            //                          [offset 44]
} // total 48 bytes

// ─── 绑定组 ─────────────────────────────────────────────────────

@group(0) @binding(0) var tile_texture: texture_2d<f32>;
@group(0) @binding(1) var tile_sampler: sampler;
@group(0) @binding(2) var<uniform> push: PushConstants;

// ─── 顶点输出 ───────────────────────────────────────────────────

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// ─── 顶点着色器 ─────────────────────────────────────────────────

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // 标准四边形（两个三角形，共 6 个顶点）：
    //   3─────4
    //   │   / │
    //   │ /   │
    //   0─────1  →  idx: 0(0,0) 1(1,0) 2(0,1) 3(1,0) 4(1,1) 5(0,1)
    let quad_pos = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let pos = quad_pos[idx];

    // 从 [0,1] 映射到世界 NDC 坐标
    // world = position + pos * size
    // 然后直接输出 NDC（假设外部已传入正确的 NDC 坐标）
    let world = push.position + pos * push.size;

    var output: VertexOutput;
    output.clip_pos = vec4<f32>(world.x, world.y, 0.0, 1.0);
    output.uv = pos * push.uv_scale + push.uv_offset;
    return output;
}

// ─── 片段着色器 ─────────────────────────────────────────────────

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(tile_texture, tile_sampler, uv);
}
