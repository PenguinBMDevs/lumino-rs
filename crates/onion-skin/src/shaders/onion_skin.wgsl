// 洋葱皮概览贴图着色器
// 全屏 quad，fragment shader 根据视口参数计算 UV 采样贴图

struct Uniform {
    area_x: f32,
    area_y: f32,
    area_w: f32,
    area_h: f32,
    time_start_ms: f32,
    time_end_ms: f32,
    key_start: f32,
    key_end: f32,
    duration_ms: f32,
    total_keys: f32,
}

@group(0) @binding(0)
var<uniform> u: Uniform;

@group(0) @binding(1)
var texture: texture_2d<f32>;

@group(0) @binding(2)
var tex_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    let positions = array(
        vec4(-1.0, -1.0, 0.0, 1.0),
        vec4( 1.0, -1.0, 0.0, 1.0),
        vec4(-1.0,  1.0, 0.0, 1.0),
        vec4( 1.0,  1.0, 0.0, 1.0),
    );
    return positions[idx];
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // 将屏幕坐标转换为卷帘区域内的局部坐标
    let local_x = pos.x - u.area_x;
    let local_y = pos.y - u.area_y;

    // 超出卷帘区域则丢弃
    if local_x < 0.0 || local_x >= u.area_w || local_y < 0.0 || local_y >= u.area_h {
        discard;
    }

    // 归一化坐标
    let norm_x = local_x / u.area_w;
    let norm_y = local_y / u.area_h;

    // 转换为时间(ms)和键位
    let time_ms = u.time_start_ms + norm_x * (u.time_end_ms - u.time_start_ms);
    let key = u.key_start + norm_y * (u.key_end - u.key_start);

    // 转换为贴图 UV（Y 轴翻转：贴图第 0 行 = key 0）
    let uv_x = time_ms / u.duration_ms;
    let uv_y = key / u.total_keys;

    return textureSample(texture, tex_sampler, vec2(uv_x, uv_y));
}
