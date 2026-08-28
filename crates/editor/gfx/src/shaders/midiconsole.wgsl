//! MidiConsole 复古终端 GPU 渲染着色器
//!
//! 全屏三角形片元着色器：根据像素坐标定位字符网格单元，从字形图集（r8 覆盖率）采样
//! 字形覆盖率，与单元前/背景色混合，最后叠加 CRT 扫描线 + 随 tick 移动的高亮扫描带。

struct Uniforms {
    grid_cols: u32,
    grid_rows: u32,
    cell_w: f32,
    cell_h: f32,
    atlas_cols: u32,
    atlas_rows: u32,
    atlas_cw: f32,
    atlas_ch: f32,
    frame_w: f32,
    frame_h: f32,
    band_center: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
};

@group(0) @binding(0) var<uniform> U: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct CellGpu {
    ch: u32,
    fg: u32,
    bg: u32,
    _pad: u32,
};
@group(0) @binding(3) var<storage, read> cells: array<CellGpu>;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VSOut {
    // 覆盖全屏的大三角形
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var o: VSOut;
    o.pos = vec4<f32>(p[vi], 0.0, 1.0);
    return o;
}

/// 字符 → 字形图集槽位（96 个 ASCII 可打印 + 1 个半块 ▌）
fn char_slot(ch: u32) -> u32 {
    if (ch >= 32u && ch <= 126u) {
        return ch - 32u;
    }
    if (ch == 0x258Cu) {
        return 96u;
    }
    return 0u; // 未知字符回退为空格
}

/// 0xRRGGBB → 线性 RGB
fn unpack_rgb(c: u32) -> vec3<f32> {
    let r = f32((c >> 16u) & 0xFFu);
    let g = f32((c >> 8u) & 0xFFu);
    let b = f32(c & 0xFFu);
    return vec3<f32>(r, g, b) / 255.0;
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = frag.xy;
    let col = u32(px.x / U.cell_w);
    let row = u32(px.y / U.cell_h);
    var color = vec3<f32>(0.0, 0.0, 0.0);
    if (col < U.grid_cols && row < U.grid_rows) {
        let idx = row * U.grid_cols + col;
        let cell = cells[idx];
        let bg = unpack_rgb(cell.bg);
        if (cell.ch == 32u) {
            color = bg;
        } else if (cell.ch == 0x258Cu) {
            // 半块字符 ▌：精确左半 fg（当前键）、右半 bg（下一键），程序化绘制、
            // 字体无关，保证键位之间无间隔、与 CPU 端 set_cell(col0,col1) 语义一致
            let lx = px.x - f32(col) * U.cell_w;
            let left = select(0.0, 1.0, lx <= U.cell_w * 0.5);
            let fg = unpack_rgb(cell.fg);
            color = mix(bg, fg, left);
        } else {
            let lx = px.x - f32(col) * U.cell_w;
            let ly = px.y - f32(row) * U.cell_h;
            let slot = char_slot(cell.ch);
            let sc = slot % U.atlas_cols;
            let sr = slot / U.atlas_cols;
            let ax = (f32(sc) + lx / U.cell_w) * U.atlas_cw;
            let ay = (f32(sr) + ly / U.cell_h) * U.atlas_ch;
            let atlas_w = f32(U.atlas_cols) * U.atlas_cw;
            let atlas_h = f32(U.atlas_rows) * U.atlas_ch;
            let uv = vec2<f32>(ax / atlas_w, ay / atlas_h);
            let cov = textureSample(atlas, samp, uv).r;
            let fg = unpack_rgb(cell.fg);
            color = mix(bg, fg, cov);
        }
    }
    // CRT：每 3 行压暗 + 随 tick 移动的高亮扫描带
    let ry = u32(px.y) % 3u;
    let scan = select(1.0, 0.82, ry == 0u);
    let dy = px.y - U.band_center;
    let band = exp(-(dy * dy) / (2.0 * 26.0 * 26.0)) * 0.28;
    color = color * scan * (1.0 + band);
    return vec4<f32>(color, 1.0);
}
