use super::cpu::{apply_crt_effect, cell_idx};
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
        next_note_id: 1,
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

/// 仅含一个短音符（CH1 C4，tick 0..240）的文档，用于过渡动画测试
fn make_short_doc() -> MidiDocument {
    let notes = vec![NoteEvent::new(0, 240, 60, 100, 0)];
    MidiDocument {
        next_note_id: 1,
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
