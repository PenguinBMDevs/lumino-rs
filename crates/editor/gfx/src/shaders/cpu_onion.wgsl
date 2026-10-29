// CPU 洋葱皮透明叠加层：把 CPU 光栅出的 RGBA8 sRGB 帧贴成屏幕空间四边形。
//
// 约定：
// - 顶点由 `vertex_index` 生成（6 顶点 = 2 个三角形），无顶点缓冲 / 实例缓冲；
// - uniform 提供「物理视口 + 物理像素矩形」，UV 0..1，nearest 采样保证 1:1 贴合；
// - 片元直接返回采样色（alpha 直通），混合由管线的 `ALPHA_BLENDING` 完成；
// - 纹理格式为 sRGB：硬件采样时做 sRGB→linear 解码，写入 sRGB 主目标时再编码回去，
//   因此 alpha = 255 的像素逐字节往返一致。

struct Uniforms {
    // xy = 物理视口尺寸（像素），zw = 保留
    viewport: vec4<f32>,
    // xy = 物理像素左上角，zw = 物理像素尺寸
    rect: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var onion_texture: texture_2d<f32>;
@group(0) @binding(2) var onion_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    // 两个三角形的 6 个角：(0,0) 左上 → (1,1) 右下
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let corner = corners[vertex_index];
    let pixel = uniforms.rect.xy + corner * uniforms.rect.zw;
    // 物理像素 → NDC（Y 轴向下，故翻转）
    let ndc = vec2<f32>(
        pixel.x / uniforms.viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / uniforms.viewport.y * 2.0,
    );

    var out: VsOut;
    // depth 恒 0.0：管线 depth_write_enabled = false / depth_compare = Always，
    // 该值不参与遮挡裁决（仅需与 depth attachment 格式兼容，避免 wgpu 校验拒绝）。
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(onion_texture, onion_sampler, in.uv);
}
