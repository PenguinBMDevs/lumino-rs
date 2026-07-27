//! 3D MIDITrail 渲染管线
//!
//! 使用实例化立方体渲染键盘与音符，包含简单漫反射光照。

struct Camera {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    ambient: f32,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) translation: vec3<f32>,
    @location(2) scale: vec3<f32>,
    @location(3) color: u32,
    @location(4) is_key: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) is_key: f32,
}

fn unpack_color(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 24u) & 0xFFu) / 255.0;
    let g = f32((packed >> 16u) & 0xFFu) / 255.0;
    let b = f32((packed >> 8u) & 0xFFu) / 255.0;
    let a = f32(packed & 0xFFu) / 255.0;
    return vec4(r, g, b, a);
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let model = mat4x4<f32>(
        vec4(in.scale.x, 0.0, 0.0, 0.0),
        vec4(0.0, in.scale.y, 0.0, 0.0),
        vec4(0.0, 0.0, in.scale.z, 0.0),
        vec4(in.translation, 1.0),
    );
    let world_pos = (model * vec4(in.position, 1.0)).xyz;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;

    let c = unpack_color(in.color);
    out.color = c.rgb;
    out.is_key = f32(in.is_key);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 根据世界坐标导数计算 flat 法线
    let dx = dpdx(in.world_pos);
    let dy = dpdy(in.world_pos);
    var normal = normalize(cross(dx, dy));
    if (length(normal) < 0.001) {
        normal = vec3(0.0, 0.0, 1.0);
    }

    let n_dot_l = max(dot(normal, camera.light_dir), 0.0);

    // 音符使用较暗环境光，琴键更亮以便看清
    let note_light = camera.ambient + (1.0 - camera.ambient) * n_dot_l;
    let key_light = 0.6 + 0.4 * n_dot_l;
    let light = mix(note_light, key_light, in.is_key);

    let color = in.color * light;
    return vec4(color, 1.0);
}
