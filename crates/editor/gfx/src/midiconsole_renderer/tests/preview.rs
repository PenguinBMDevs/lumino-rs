//! MidiConsole GPU 渲染器预览测试
//!
//! 在可用 GPU/软件适配器上渲染一帧复古终端，写入 `target/midiconsole_gpu_preview.png`，
//! 并断言输出包含可见像素（验证 GPU 字形图集 + 全屏着色 + CRT 后处理管线真实工作）。

use super::super::*;
use std::path::Path;

fn set(grid: &mut [CellGpu], row: usize, col: usize, ch: char, fg: u32, bg: u32) {
    let idx = row * GRID_COLS + col;
    grid[idx] = CellGpu {
        ch: ch as u32,
        fg,
        bg,
        _pad: 0,
    };
}

#[test]
fn test_gpu_renders_preview_png() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = futures::executor::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .expect("测试需要可用的 wgpu 适配器（GPU 或软件后端）");
    let (device, queue) = futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("midiconsole_test_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败");

    // 输出帧 1480×800，单元 10×20（与 CPU 预览一致）
    let cell_w = 10u32;
    let cell_h = 20u32;
    let width = (GRID_COLS as u32) * cell_w;
    let height = (GRID_ROWS as u32) * cell_h;
    let mut renderer = MidiconsoleRenderer::new(&device, &queue, width, height);

    let bg_dark: u32 = pack_rgb(12, 12, 14);
    let fg_light: u32 = pack_rgb(205, 205, 185);
    let key_black: u32 = pack_rgb(40, 40, 46);
    let key_white: u32 = pack_rgb(60, 60, 68);
    let warm: u32 = pack_rgb(90, 200, 255);

    let mut grid = vec![
        CellGpu { ch: 32, fg: 0, bg: bg_dark, _pad: 0 };
        GRID_COLS * GRID_ROWS
    ];

    // 表头
    let header = "LUMINO MIDICONSOLE";
    for (i, c) in header.chars().enumerate() {
        set(&mut grid, 0, 1 + i, c, fg_light, bg_dark);
    }
    for (i, c) in "> PLAYING".chars().enumerate() {
        set(&mut grid, 0, GRID_COLS - 10 + i, c, warm, bg_dark);
    }

    // 控制面板标签行
    let labels = "PC VOL EXP PAN P.BEND P.RANGE MOD HOLD CUT RESO ATT DEC REL";
    for (i, c) in labels.chars().enumerate() {
        set(&mut grid, 2, 71 + i, c, fg_light, bg_dark);
    }

    // 一个键盘条（CH01）：左侧标签 + 64 个半块键，黑白交替并以暖色高亮若干
    let row_kb = 6usize;
    for (i, c) in "CH01".chars().enumerate() {
        set(&mut grid, row_kb, 1 + i, c, fg_light, bg_dark);
    }
    for k in 0..64usize {
        let (fg, bg) = if k % 12 == 1 || k % 12 == 3 || k % 12 == 6 || k % 12 == 8 || k % 12 == 10
        {
            (key_black, bg_dark)
        } else {
            (key_white, bg_dark)
        };
        let fg = if k < 8 { warm } else { fg };
        set(&mut grid, row_kb, 5 + k, '\u{258C}', fg, bg);
    }

    // 保证可见性：底部整行用亮色背景填充（即便字体缺失也不会全黑）
    let bg_bright: u32 = pack_rgb(30, 40, 60);
    for c in 0..GRID_COLS {
        set(&mut grid, GRID_ROWS - 1, c, ' ', fg_light, bg_bright);
    }

    let rgba = renderer.render_to_rgba(&device, &queue, &grid, 40);

    // 统计非黑像素 / 明亮字形像素 / 底部亮条背景像素
    let mut non_black = 0usize;
    let mut bright = 0usize;
    let mut bar_color = 0usize;
    for px in rgba.chunks_exact(4) {
        if px[0] > 4 || px[1] > 4 || px[2] > 4 {
            non_black += 1;
        }
        if px[0] > 150 || px[1] > 150 || px[2] > 150 {
            bright += 1;
        }
        if (px[0] as i32 - 30).abs() < 8
            && (px[1] as i32 - 40).abs() < 8
            && (px[2] as i32 - 60).abs() < 8
        {
            bar_color += 1;
        }
    }
    assert!(
        non_black > 1000,
        "GPU 渲染应包含大量可见像素，但仅 {non_black} 个非黑像素"
    );
    assert!(
        bright > 500,
        "应渲染出明亮字形（文本/琴键），但仅 {bright} 个亮像素"
    );
    assert!(
        bar_color > 1000,
        "底部亮条背景应被渲染，但仅 {bar_color} 个匹配像素"
    );

    // 写 PNG
    let out_dir = Path::new("target");
    std::fs::create_dir_all(out_dir).expect("创建 target 目录失败");
    let path = out_dir.join("midiconsole_gpu_preview.png");
    let file = std::fs::File::create(&path).expect("创建预览 PNG 失败");
    let mut enc = png::Encoder::new(file, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("写 PNG 头失败");
    writer
        .write_image_data(&rgba)
        .expect("写 PNG 像素失败");
    eprintln!(
        "已写入 GPU 渲染预览: {}（{}×{}，非黑像素 {non_black}）",
        path.display(),
        width,
        height
    );
}
