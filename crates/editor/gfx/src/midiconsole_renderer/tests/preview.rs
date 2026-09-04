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
    let (device, queue) =
        futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
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
    let key_black: u32 = pack_rgb(72, 76, 92);
    let key_white: u32 = pack_rgb(104, 110, 126);
    let warm: u32 = pack_rgb(90, 200, 255);

    let mut grid = vec![
        CellGpu {
            ch: 32,
            fg: 0,
            bg: bg_dark,
            _pad: 0
        };
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
    // 状态行
    let stats = "SPD 1.00x  BPM 120.0  4/4  TPQ 480  TICK 0  NOTES 128  EVENTS 64";
    for (i, c) in stats.chars().enumerate() {
        set(&mut grid, 1, 1 + i, c, fg_light, bg_dark);
    }

    // 控制面板标签行（row 2），位于键盘条右侧（同行横向对齐）
    let labels = [
        "PC", "VOL", "EXP", "PAN", "P.BEND", "P.RANGE", "MOD", "HOLD", "CUT", "RESO", "ATT", "DEC",
        "REL",
    ];
    let field_cols = [
        71usize, 76, 81, 86, 91, 99, 107, 112, 117, 122, 127, 132, 137,
    ];
    for f in 0..13usize {
        for (i, c) in labels[f].chars().enumerate() {
            set(&mut grid, 2, field_cols[f] + i, c, fg_light, bg_dark);
        }
    }

    // 每通道：键盘条（左，cols 5..68）+ 控制数据（右，col 71+）在同一行，横向对齐；
    // 与 CPU 端 render() 的「数据在键盘右侧、同行横向对齐」布局一致
    let draw_kb = |grid: &mut [CellGpu], row: usize, label: &str, lit: &[usize]| {
        for (i, c) in label.chars().enumerate() {
            set(grid, row, 1 + i, c, fg_light, bg_dark);
        }
        for i in 0..64usize {
            let k0 = i * 2;
            let k1 = i * 2 + 1;
            let is_black =
                |k: usize| k % 12 == 1 || k % 12 == 3 || k % 12 == 6 || k % 12 == 8 || k % 12 == 10;
            let col0 = if lit.contains(&k0) {
                warm
            } else if is_black(k0) {
                key_black
            } else {
                key_white
            };
            let col1 = if lit.contains(&k1) {
                warm
            } else if is_black(k1) {
                key_black
            } else {
                key_white
            };
            set(grid, row, 5 + i, '\u{258C}', col0, col1);
        }
    };
    for ch in 0..16usize {
        let kb_row = 3 + ch * 2;
        // 数据（与键盘同行，右侧 col 71+）
        let vals = [
            format!("{:>3}", ch + 1),
            format!("{:>3}", 100usize.wrapping_sub(ch)),
            format!("{:>3}", 64 + ch % 20),
            format!("{:>3}", (ch * 7) % 128),
            format!("{:>4}", (ch as i32) * 100 - 200),
            format!("{:>3}", 24),
            format!("{:>3}", ch * 5),
            format!("{:>3}", (ch % 2) * 64),
            format!("{:>3}", 40 + ch),
            format!("{:>3}", 60usize.wrapping_sub(ch)),
            format!("{:>3}", 10),
            format!("{:>3}", 20 + ch),
            format!("{:>3}", 30),
        ];
        for f in 0..13usize {
            for (i, c) in vals[f].chars().enumerate() {
                set(&mut grid, kb_row, field_cols[f] + i, c, fg_light, bg_dark);
            }
        }
        // 键盘
        let lit_keys: Vec<usize> = (0..128usize)
            .filter(|k| (k % 12 == ch % 12) || (k % 13 == (ch + 3) % 13))
            .collect();
        let label = format!("CH{:02}", ch + 1);
        draw_kb(&mut grid, kb_row, &label, &lit_keys);
    }
    // ALL 合并键盘条（row 35，仅键盘）
    draw_kb(
        &mut grid,
        3 + 16 * 2,
        "ALL",
        &[0, 12, 24, 36, 48, 60, 72, 84, 96, 108, 120],
    );

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
    for px in rgba.as_chunks::<4>().0 {
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
    writer.write_image_data(&rgba).expect("写 PNG 像素失败");
    eprintln!(
        "已写入 GPU 渲染预览: {}（{}×{}，非黑像素 {non_black}）",
        path.display(),
        width,
        height
    );
}
