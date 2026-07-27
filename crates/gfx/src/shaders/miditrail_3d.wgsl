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
    @location(1) normal: vec3<f32>,
    @location(2) translation: vec3<f32>,
    @location(3) scale: vec3<f32>,
    @location(4) color: u32,
    @location(5) is_key: u32,
    @location(6) press_factor: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) is_key: f32,
}

fn unpack_color(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 24u) & 0xFFu) / 255.0;
    let g = f32((packed >> 16u) & 0xFFu) / 255.0;
    let b = f32((packed >> 8u) & 0xFFu) / 255.0;
    let a = f32(packed & 0xFFu) / 255.0;
    return vec4(r, g, b, a);
}

fn key_color_factor(normal: vec3<f32>) -> f32 {
    let n = normalize(normal);
    let is_top = n.y > 0.75;
    let is_front = n.z > 0.75;
    let is_back = n.z < -0.75;
    let is_bottom = n.y < -0.75;

    if (is_top) {
        // 顶层：高亮白色 / 高亮激活色
        return 1.0;
    } else if (is_front) {
        // 面向摄像机的正面：保留淡灰
        return 0.85;
    } else if (is_back || is_bottom) {
        return 0.3;
    } else {
        // 左右侧面
        return 0.7;
    }
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var translation = in.translation;
    if (in.is_key == 1u) {
        // 按下时琴键整体向下凹陷，深度约为键自身高度的 0.5
        translation.y -= in.press_factor * in.scale.y * 0.5;
    }

    let model = mat4x4<f32>(
        vec4(in.scale.x, 0.0, 0.0, 0.0),
        vec4(0.0, in.scale.y, 0.0, 0.0),
        vec4(0.0, 0.0, in.scale.z, 0.0),
        vec4(translation, 1.0),
    );
    let world_pos = (model * vec4(in.position, 1.0)).xyz;
    let world_normal = (model * vec4(in.normal, 0.0)).xyz;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.normal = world_normal;

    let c = unpack_color(in.color);
    out.color = c.rgb;
    out.is_key = f32(in.is_key);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var normal = normalize(in.normal);
    if (length(normal) < 0.001) {
        normal = vec3(0.0, 0.0, 1.0);
    }

    var color = in.color;
    if (in.is_key > 0.5) {
        // 琴键按面着色：顶层高亮，正面淡灰，背面/底面压暗
        let factor = key_color_factor(normal);
        color = in.color * factor;
    } else {
        let n_dot_l = max(dot(normal, camera.light_dir), 0.0);
        color = in.color * (camera.ambient + (1.0 - camera.ambient) * n_dot_l);
    }
    return vec4(color, 1.0);
}
