//! 3D MIDITrail Aura 光环渲染管线
//!
//! 在琴键前缘（Z 方向头部）围绕音符立方体绘制一个光环，使用附加混合实现发光效果。
//! 光环半径随音符事件实时变化（按下闪光放大 + 临近结束收缩，见
//! `miditrail_renderer/instances.rs` 的 `build_aura_instances`），
//! 每实例的 `size` 字段即当前帧的动画半径。

struct Camera {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    ambient: f32,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var aura_sampler: sampler;
@group(0) @binding(2) var aura_texture: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) size: f32,
    @location(3) pos: f32,
    @location(4) color: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
}

// 键盘背部（音符落入键盘的那一端），光环位于键后缘稍后方
const AURA_Z: f32 = -0.006;
// 与音符立方体中心对齐的 y 坐标
const AURA_Y: f32 = 0.0005;

fn unpack_color(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 24u) & 0xFFu) / 255.0;
    let g = f32((packed >> 16u) & 0xFFu) / 255.0;
    let b = f32((packed >> 8u) & 0xFFu) / 255.0;
    let a = f32(packed & 0xFFu) / 255.0;
    return vec4(r, g, b, a);
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let world_pos = vec3(
        in.pos + in.position.x * in.size,
        AURA_Y + in.position.y * in.size,
        AURA_Z,
    );

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4(world_pos, 1.0);
    out.uv = in.uv;
    let c = unpack_color(in.color);
    out.color = c.rgb;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(aura_texture, aura_sampler, in.uv);
    let color = in.color * tex.a;
    return vec4(color, tex.a);
}
