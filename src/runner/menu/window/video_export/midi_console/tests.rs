use super::cpu::{apply_crt_effect, cell_idx, fill_rect};
use super::*;
use lumino_midi_loader::{ChunkedList, MidiDocument, NoteEvent, TrackManager};

fn make_doc() -> MidiDocument {
    // 跨多个通道、在 tick=960 处全部处于发声状态的音符，
    // 让预览图里 CH01..CH06 与 ALL 行的键盘条同时点亮。
    let notes = vec![
        NoteEvent::new(0, 1920, 60, 100, 0),   // CH1 C4
        NoteEvent::new(0, 1920, 64, 100, 0),   // CH1 E4
        NoteEvent::new(0, 1920, 67, 100, 0),   // CH1 G4
        NoteEvent::new(100, 1900, 55, 100, 1), // CH2 G3
        NoteEvent::new(100, 1900, 59, 100, 1), // CH2 B3
        NoteEvent::new(200, 1700, 72, 100, 2), // CH3 C5
        NoteEvent::new(300, 1600, 48, 100, 3), // CH4 C3
        NoteEvent::new(400, 1500, 76, 100, 4), // CH5 E5
        NoteEvent::new(500, 1400, 81, 100, 5), // CH6 A5
        NoteEvent::new(0, 1920, 72, 100, 7),   // CH8 C5（验证中段通道）
        NoteEvent::new(0, 1920, 84, 100, 15),  // CH16 C6（验证最高通道）
    ];
    let mut list: Vec<NoteEvent> = notes;
    list.sort_unstable_by_key(|n| n.start_tick);
    MidiDocument {
        notes: vec![ChunkedList::from_sorted(list)],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T1".into())],
        total_ticks: 1920,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    }
}

#[test]
fn test_render_produces_grid_cells() {
    let doc = make_doc();
    let cfg = MidiConsoleRenderConfig::from(&MidiConsoleConfig::default());
    let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);
    let mut grid = vec![Cell::blank(); (COLS * ROWS) as usize];
    renderer.render(&mut grid, &doc, 240, 480, 60);

    // 应有大量非空格单元（键盘条 ▌ + 文本）
    let mut non_empty = 0usize;
    for c in &grid {
        if c.ch != ' ' {
            non_empty += 1;
        }
    }
    assert!(
        non_empty > 1000,
        "字符网格应产生大量非空格，实际 {non_empty}"
    );
}

#[test]
fn test_render_detects_active_channel_keys() {
    let doc = make_doc();
    let cfg = MidiConsoleRenderConfig::from(&MidiConsoleConfig::default());
    let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);
    let mut grid = vec![Cell::blank(); (COLS * ROWS) as usize];
    // tick=240：验证跨通道独立检测（CH1 / CH2 / CH8 / CH16 各自按键）
    renderer.render(&mut grid, &doc, 240, 480, 60);
    assert!(renderer.pressed[0][60], "CH1 C4 应被按下");
    assert!(renderer.pressed[1][55], "CH2 G3 应被按下");
    assert!(renderer.pressed[7][72], "CH8 C5 应被按下（中段通道）");
    assert!(renderer.pressed[15][84], "CH16 C6 应被按下（最高通道）");
    assert!(renderer.note_count >= 4, "已计数的音符应 >= 4");
}

/// 渲染一帧并导出 PNG 预览图，供人工查看 MidiConsole 风格效果。
#[test]
fn test_render_preview_png() {
    use std::io::Write;

    let doc = make_doc();
    let cfg = MidiConsoleRenderConfig::from(&MidiConsoleConfig::default());
    let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);

    // 整数倍 cell 比例（约 1:2，贴近真实终端），148×40 → 10×20 = 1480×800
    let cell = 10u32;
    let w = COLS * cell;
    let h = ROWS * (cell * 2);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    // 取 tick=960（多个通道音符同时发声），键盘条应点亮为暖色
    render_midicomsole_frame(MidiConsoleFrameArgs {
        renderer: &mut renderer,
        frame: &mut frame,
        frame_width: w,
        frame_height: h,
        document: &doc,
        tick: 960,
        ppq: 480,
        fps: 60,
    });

    assert!(frame.iter().any(|&v| v != 0), "预览帧不应全黑");

    // ── 字形坐标正确性验证（闭环证据）──
    // 修复前 draw 回调的 px/py 被误当绝对坐标，所有字形堆在帧左上角 (0,0) 互相覆盖。
    // 修复后：表头文字应出现在正确格子，左上角第 0 列（header 行此处为空格）应保持背景。
    let cell_w_i = cell as usize;
    let cell_h_i = (cell * 2) as usize;
    let fw_us = w as usize;
    // (1) 左上角第 0 列不应被字形堆满
    let mut left_col_light = 0usize;
    for y in 0..cell_h_i {
        for x in 0..cell_w_i {
            let di = (y * fw_us + x) * 4;
            if frame[di] > 60 {
                left_col_light += 1;
            }
        }
    }
    assert!(
        left_col_light < 50,
        "左上角第0列不应被字形堆满（坐标偏移修复后），实际亮像素 {left_col_light}"
    );
    // (2) 表头整行应有文字亮起（字形已正确定位）
    let mut header_light = 0usize;
    for y in 0..cell_h_i {
        for x in 0..fw_us {
            let di = (y * fw_us + x) * 4;
            if frame[di] > 60 {
                header_light += 1;
            }
        }
    }
    assert!(
        header_light > 200,
        "表头行应有文字亮起（字形已正确定位），实际亮像素 {header_light}"
    );

    // BGRA -> RGBA 后写出 PNG
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    let _ = std::fs::create_dir_all(&out_dir);
    let path = out_dir.join("midi_console_preview.png");
    let file = std::fs::File::create(&path).expect("创建预览 PNG 文件");
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .expect("写入 PNG 头")
        .into_stream_writer()
        .expect("创建 PNG 流写入器");
    for pix in frame.as_chunks::<4>().0 {
        let (b, g, r, a) = (pix[0], pix[1], pix[2], pix[3]);
        writer.write_all(&[r, g, b, a]).expect("写入 PNG 像素");
    }
    writer.finish().expect("结束 PNG 写入");

    // 以字符网格形式输出文本预览，便于无图环境核对布局
    let grid = {
        let mut g = vec![Cell::blank(); (COLS * ROWS) as usize];
        let mut r2 = MidiConsoleRenderer::new(&doc, &cfg);
        r2.render(&mut g, &doc, 960, 480, 60);
        g
    };
    println!("---- MidiConsole 字符网格预览 (148x40, . = 空格) ----");
    for r in 0..ROWS {
        let mut line = String::new();
        for c in 0..COLS {
            let ch = grid[cell_idx(r, c)].ch;
            line.push(if ch == ' ' { '.' } else { ch });
        }
        println!("{line}");
    }
    println!("------------------------------------------------------");
    println!("MidiConsole 预览图已写出: {}", path.display());
}

/// 按键亮度水平应随按下/松开平滑过渡（亮起渐变 + 熄灭渐变），而非瞬间跳变
#[test]
fn test_key_level_animates() {
    let doc = make_short_doc();
    let cfg = MidiConsoleRenderConfig {
        render_backend: MidiConsoleBackend::Gpu,
        show_control_panel: true,
        keyboard_fade_frames: 30,
        control_fade_frames: 30,
        warm_key_color: [234, 234, 208],
    };
    let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);
    let mut grid = vec![Cell::blank(); (COLS * ROWS) as usize];
    // 按住（tick=100，CH1 C4 发声中）：连续多帧后亮度应平滑趋近 1
    for _ in 0..30 {
        renderer.render(&mut grid, &doc, 100, 480, 60);
    }
    assert!(
        renderer.key_level[1][60] > 0.8,
        "按住时按键亮度应平滑趋近 1（亮起渐变动画），实际 {}",
        renderer.key_level[1][60]
    );
    // 松开（tick=300 > 结束 240）：连续多帧后亮度应淡出趋近 0
    for _ in 0..40 {
        renderer.render(&mut grid, &doc, 300, 480, 60);
    }
    assert!(
        renderer.key_level[1][60] < 0.1,
        "松开后按键亮度应淡出趋近 0（熄灭渐变动画），实际 {}",
        renderer.key_level[1][60]
    );
}

/// CRT 后处理应产生扫描线压暗，且随 tick 改变帧内容（动态发光扫描线）
#[test]
fn test_crt_scanline_darkens() {
    let w = 100u32;
    let h = 100u32;
    let mut frame = vec![0u8; (w * h * 4) as usize];
    for (i, px) in frame.iter_mut().enumerate() {
        *px = if i % 4 == 3 { 255 } else { 200 };
    }
    let original = frame.clone();
    // tick=10 时移动亮带中心约在 y=60，远离顶行，便于比较扫描线压暗
    apply_crt_effect(&mut frame, w as usize, h as usize, 10);
    assert_ne!(frame, original, "CRT 后处理应改变像素（扫描线/亮带生效）");
    // 扫描线行（y%3==0，如 y=0）应比相邻非扫描线行（y=1）暗
    let r_scan = frame[0] as f32;
    let r_nonscan = frame[(w as usize) * 4] as f32;
    assert!(
        r_scan < r_nonscan,
        "扫描线行应比非扫描线行暗（r_scan={r_scan}, r_nonscan={r_nonscan}）"
    );
}

/// 字形栅格缓存必须与逐帧 `outline_glyph` 直栅格逐像素一致（像素等价护栏）
#[test]
fn test_cached_glyph_matches_direct_raster() {
    use ab_glyph::{Font as _, PxScale};

    let Some(font) = super::font::load_monospace_font() else {
        return; // 无可用系统字体（精简镜像）：跳过
    };
    let scale = PxScale { x: 27.0, y: 27.0 };
    let (fw, fh) = (80usize, 40usize);
    let bg = [20u8, 20, 20];
    let fg = [200u8, 180, 60];
    let mut got = vec![0u8; fw * fh * 4];
    for px in got.as_chunks_mut::<4>().0 {
        *px = [bg[0], bg[1], bg[2], 255];
    }
    let mut want = got.clone();

    // 多字符 × 多亚像素相位（小数部分不同 → 不同栅格相位）
    for (ci, ch) in ['A', '5', '▌', 'g'].into_iter().enumerate() {
        for k in 0..4 {
            let bx = 3.37 + k as f32 * 13.0 + ci as f32 * 0.13;
            let by = 28.0 + ci as f32 * 0.07;

            // 缓存路径（生产实现）
            let cached = super::glyph_cache::cached_glyph(&font, scale, ch, bx, by);
            let b_min = (
                (cached.base_min.0 + bx.trunc()) as i32,
                (cached.base_min.1 + by.trunc()) as i32,
            );
            for &(ox, oy, cov) in &cached.samples {
                draw_blend(&mut got, fw, b_min.0 + ox, b_min.1 + oy, bg, fg, cov);
            }

            // 直栅格路径（改造前实现）
            let glyph = font
                .glyph_id(ch)
                .with_scale_and_position(scale, ab_glyph::point(bx, by));
            if let Some(outline) = font.outline_glyph(glyph) {
                let b = outline.px_bounds();
                outline.draw(|px, py, cov| {
                    draw_blend(
                        &mut want,
                        fw,
                        (px as f32 + b.min.x).round() as i32,
                        (py as f32 + b.min.y).round() as i32,
                        bg,
                        fg,
                        cov,
                    );
                });
            }
        }
    }
    assert_eq!(got, want, "缓存字形与直栅格输出不一致");
}

/// 测试用混合（与 `cpu::blend` 同式；裁剪规则与生产一致）
fn draw_blend(frame: &mut [u8], fw: usize, x: i32, y: i32, bg: [u8; 3], fg: [u8; 3], cov: f32) {
    if x < 0 || y < 0 {
        return;
    }
    let di = (y as usize * fw + x as usize) * 4;
    if di + 3 >= frame.len() {
        return;
    }
    let a = cov.clamp(0.0, 1.0);
    let blend = |b: u8, f: u8| (f as f32 * a + b as f32 * (1.0 - a)).clamp(0.0, 255.0) as u8;
    frame[di] = blend(bg[0], fg[0]);
    frame[di + 1] = blend(bg[1], fg[1]);
    frame[di + 2] = blend(bg[2], fg[2]);
    frame[di + 3] = 255;
}

/// 覆盖率混合（含 0 / ≥1 两端短路）必须与逐通道原式逐位一致
#[test]
fn test_coverage_word_matches_direct_blend() {
    let fg = [200u8, 180, 60];
    let bg = [20u8, 20, 20];
    let fg_f = fg.map(|v| v as f32);
    let bg_f = bg.map(|v| v as f32);
    let fg_word = super::cpu::pack_bgra(fg);
    let bg_word = super::cpu::pack_bgra(bg);

    let mut covers = vec![-0.5f32, 0.0, 1.0, 1.5];
    covers.extend((0..=1000).map(|k| k as f32 / 1000.0));
    for cov in covers {
        let bytes = super::cpu::coverage_word(fg_f, bg_f, fg_word, bg_word, cov).to_le_bytes();
        let a = cov.clamp(0.0, 1.0);
        for ch in 0..3 {
            // 原式（与改造前逐像素 blend 相同）
            let want = (fg_f[ch] * a + bg_f[ch] * (1.0 - a)).clamp(0.0, 255.0) as u8;
            assert_eq!(bytes[ch], want, "cov={cov} 通道 {ch} 不一致");
        }
        assert_eq!(bytes[3], 255, "alpha 必须不透明");
    }
}

/// CRT 查表实现必须与逐像素原式逐位一致（像素等价护栏）
#[test]
fn test_crt_lut_matches_direct_formula() {
    let w = 37usize;
    let h = 53usize;
    let mut frame = vec![0u8; w * h * 4];
    for (i, v) in frame.iter_mut().enumerate() {
        *v = if i % 4 == 3 {
            255
        } else {
            ((i * 37) % 256) as u8
        };
    }
    let expected = frame.clone();
    apply_crt_effect(&mut frame, w, h, 123);

    let scan_period = 3usize;
    let scan_dark = 0.82f32;
    let band_center = (123.0 * 6.0f32) % h as f32;
    let band_width = 26.0f32;
    let band_strength = 0.28f32;
    for y in 0..h {
        let scan = if y % scan_period == 0 { scan_dark } else { 1.0 };
        let dy = (y as f32 - band_center).abs();
        let glow = (-(dy * dy) / (2.0 * band_width * band_width)).exp() * band_strength;
        let factor = scan * (1.0 + glow);
        for x in 0..w {
            let di = (y * w + x) * 4;
            for ch in 0..3 {
                let want = (expected[di + ch] as f32 * factor).clamp(0.0, 255.0) as u8;
                assert_eq!(frame[di + ch], want, "像素 ({x},{y}) 通道 {ch} 不一致");
            }
            assert_eq!(frame[di + 3], expected[di + 3], "alpha 不应被 CRT 修改");
        }
    }
}

/// 行内整段填充必须与逐像素写等价（含裁剪、小数边界、窄/宽两条实现分支）
#[test]
fn test_fill_rect_row_chunks_match_per_pixel() {
    let (fw, fh) = (41usize, 29usize);
    let color = [10u8, 20, 30];
    // 9.6px 宽 → memcpy 分支；31.4px 宽 → 逐像素分支；2.2px 起点为小数
    for rect in [
        (3.4f32, 2.7f32, 9.6f32, 7.2f32),
        (5.6f32, 1.1f32, 31.4f32, 4.9f32),
        (-3.0f32, -2.0f32, 6.5f32, 5.5f32),
    ] {
        let mut got = vec![0u8; fw * fh * 4];
        fill_rect(&mut got, (fw, fh), rect, color);

        let mut want = vec![0u8; fw * fh * 4];
        let (x0, y0, rw, rh) = rect;
        let x0 = x0.max(0.0) as i64;
        let y0 = y0.max(0.0) as i64;
        let x1 = ((x0 as f32 + rw) as i64).min(fw as i64);
        let y1 = ((y0 as f32 + rh) as i64).min(fh as i64);
        for y in y0..y1 {
            let row = y as usize * fw;
            for x in x0..x1 {
                let di = (row + x as usize) * 4;
                want[di] = color[2];
                want[di + 1] = color[1];
                want[di + 2] = color[0];
                want[di + 3] = 255;
            }
        }
        assert_eq!(got, want, "rect={rect:?} 行整段填充与逐像素写结果不一致");
    }
}

/// 仅含一个短音符（CH1 C4，tick 0..240）的文档，用于过渡动画测试
fn make_short_doc() -> MidiDocument {
    let notes = vec![NoteEvent::new(0, 240, 60, 100, 0)];
    MidiDocument {
        notes: vec![ChunkedList::from_sorted(notes)],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T1".into())],
        total_ticks: 240,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    }
}
