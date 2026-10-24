// splat 第二段：把深度缓冲里的胜者索引解析成颜色。
//
// source 按 chunk 切片绑定，因此这里只接受落在本 chunk 的胜者；所有 chunk
// 各跑一遍全屏像素（像素量固定 2M 左右，chunk 数通常 1~2）。
// composite 只读本 pass 的输出，不再整段绑定 source。

struct NoteInstance {
    start: f32,
    length: f32,
    attrs: u32,
};

fn note_track(attrs: u32) -> u32 { return (attrs >> 8u) & 0xFFFFu; }
fn note_color(attrs: u32) -> vec4<f32> {
    let idx = max(note_track(attrs), 1u) - 1u;
    let colors = array<vec4<f32>, 12>(
        vec4<f32>(0.7843137, 0.3137255, 0.3137255, 1.0),
        vec4<f32>(0.3137255, 0.7843137, 0.4705882, 1.0),
        vec4<f32>(0.3137255, 0.4705882, 0.8627451, 1.0),
        vec4<f32>(0.8627451, 0.7843137, 0.3137255, 1.0),
        vec4<f32>(0.7843137, 0.3921569, 0.7843137, 1.0),
        vec4<f32>(0.3137255, 0.7843137, 0.7843137, 1.0),
        vec4<f32>(0.9411765, 0.5882353, 0.3137255, 1.0),
        vec4<f32>(0.7058824, 0.7058824, 0.7058824, 1.0),
        vec4<f32>(0.9019608, 0.3921569, 0.3921569, 1.0),
        vec4<f32>(0.3921569, 0.7058824, 0.3921569, 1.0),
        vec4<f32>(0.3921569, 0.3921569, 0.7843137, 1.0),
        vec4<f32>(0.7843137, 0.7058824, 0.3921569, 1.0),
    );
    return colors[idx % 12u];
}

struct SplatParams {
    width: u32,
    height: u32,
    scale_x: f32,
    scale_y: f32,
    chunk_base: u32,
    chunk_count: u32,
    _padding: vec2<u32>,
};

const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);

@group(0) @binding(0) var<storage, read> all_instances: array<NoteInstance>;
@group(0) @binding(1) var<storage, read> depth: array<u32>;
@group(0) @binding(2) var<storage, read_write> winner_color: array<u32>;
@group(0) @binding(3) var<uniform> params: SplatParams;

fn pack_rgba(color: vec4<f32>) -> u32 {
    let r = u32(clamp(color.r, 0.0, 1.0) * 255.0 + 0.5);
    let g = u32(clamp(color.g, 0.0, 1.0) * 255.0 + 0.5);
    let b = u32(clamp(color.b, 0.0, 1.0) * 255.0 + 0.5);
    return (r << 24u) | (g << 16u) | (b << 8u) | 255u;
}

@compute @workgroup_size(256)
fn resolve(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = gid.x + gid.y * 65535u * 256u;
    if (pixel >= params.width * params.height) {
        return;
    }
    let packed = depth[pixel];
    if (packed == 0u) {
        return;
    }
    let index = 0x7FFFFFFFu - (packed & 0x7FFFFFFFu);
    if (index < params.chunk_base || index >= params.chunk_base + params.chunk_count) {
        return;
    }
    let inst = all_instances[index - params.chunk_base];
    let is_main = (packed & 0x80000000u) != 0u;
    var color = note_color(inst.attrs);
    if (is_main) {
        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
    }
    winner_color[pixel] = pack_rgba(color);
}
