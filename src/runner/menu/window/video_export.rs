//! 视频导出辅助函数
//!
//! 包含视频帧渲染参数构建、键盘贴图生成、标尺小节号合成等工具函数。

use lumino_gfx::{
    ARRANGEMENT_PALETTE, GridViewParams, HiResConfig, HiResRenderMode, NoteInstance, OnionSkinNote,
    RenderParams, compute_midi_hash, generate_grid_instances, generate_ruler_instances,
    is_black_key, pack_color,
};

// ── 时间计算 ──

/// 从 tempo_changes 计算总时长（秒）
pub(super) fn compute_duration_secs(
    tempo_changes: &[(u32, f32)],
    total_ticks: u32,
    ppq: u32,
) -> f64 {
    if tempo_changes.is_empty() || ppq == 0 {
        return total_ticks as f64 / ppq.max(1) as f64 * 0.5; // 120 BPM
    }
    let mut secs = 0.0;
    let mut prev_tick = 0u32;
    let mut prev_bpm = tempo_changes[0].1 as f64;
    for &(tick, bpm) in tempo_changes {
        if tick > prev_tick {
            let delta_ticks = (tick - prev_tick) as f64;
            secs += delta_ticks / ppq as f64 * 60.0 / prev_bpm;
        }
        prev_tick = tick;
        prev_bpm = bpm as f64;
    }
    if total_ticks > prev_tick {
        let delta_ticks = (total_ticks - prev_tick) as f64;
        secs += delta_ticks / ppq as f64 * 60.0 / prev_bpm;
    }
    secs
}

/// 从秒转换到 tick
pub(super) fn seconds_to_tick(secs: f64, tempo_changes: &[(u32, f32)], ppq: u32) -> u32 {
    if tempo_changes.is_empty() || ppq == 0 {
        return (secs * ppq.max(1) as f64 * 2.0) as u32; // 120 BPM
    }
    let mut remaining = secs;
    let mut prev_tick = 0u32;
    let mut prev_bpm = tempo_changes[0].1 as f64;
    for &(tick, bpm) in tempo_changes {
        if tick > prev_tick {
            let delta_ticks = (tick - prev_tick) as f64;
            let delta_secs = delta_ticks / ppq as f64 * 60.0 / prev_bpm;
            if remaining <= delta_secs {
                return prev_tick + (remaining * ppq as f64 * prev_bpm / 60.0) as u32;
            }
            remaining -= delta_secs;
        }
        prev_tick = tick;
        prev_bpm = bpm as f64;
    }
    prev_tick + (remaining * ppq as f64 * prev_bpm / 60.0) as u32
}

// ── 渲染参数构建 ──

/// 构建视频导出帧的 RenderParams
///
/// 包含编辑区域 UI 元素（网格线、标尺、键盘），
/// Y 向缩放覆盖 128 键标准 MIDI 键盘，
/// X 向缩放使可见视口内恰好 4 个小节。
pub(super) fn build_video_render_params(
    width: u32,
    height: u32,
    tick: u32,
    document: &lumino_midi_loader::MidiDocument,
    ppq: u32,
    _key_count: u16,
) -> RenderParams {
    // 视频导出始终使用标准 128 键 MIDI 键盘
    const KEY_COUNT: u16 = 128;

    let keyboard_width = 60.0f32;
    let ruler_height = 30.0f32;
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;

    // X 向缩放：视口 tick 范围 = 4 小节
    let viewport_tick_span = (ppq * 16).max(1) as f32;
    let zoom_x = (w - keyboard_width) / viewport_tick_span;

    // Y 向缩放：覆盖整个键盘（固定 128 键）
    let key_count_f = KEY_COUNT as f32;
    let zoom_y = (h - ruler_height) / key_count_f;

    // scroll_x 必须乘以 zoom_x，因为 note shader 使用 tick * zoom_x - scroll_x
    let scroll_x = tick as f32 * zoom_x;
    let scroll_y = 0.0f32;

    // 1. 生成网格线实例
    let grid_params = GridViewParams {
        viewport_width: w,
        viewport_height: h,
        keyboard_width,
        ruler_height,
        scroll_x,
        scroll_y,
        zoom_x,
        zoom_y,
    };
    let grid_instances = generate_grid_instances(&grid_params);

    // 2. 生成标尺实例
    let ruler_instances =
        generate_ruler_instances(w, keyboard_width, ruler_height, scroll_x, zoom_x);

    // 3. 键盘使用 CPU 贴图方式（视频导出线程中 composite），GPU 不渲染键盘实例
    let keyboard_instances = Vec::new();

    // 4. 收集可见音符，按音轨分配颜色
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);
    let mut note_instances = Vec::new();
    for (track_idx, notes) in document.notes.iter().enumerate() {
        let color = ARRANGEMENT_PALETTE[track_idx % ARRANGEMENT_PALETTE.len()];
        let color_packed = pack_color([color[0], color[1], color[2], 1.0]);
        for n in notes {
            if n.end_tick >= tick_start && n.start_tick <= tick_end {
                note_instances.push(NoteInstance {
                    position: [n.start_tick as f32, n.key as f32],
                    size_x: (n.length() as f32).max(1.0),
                    color_packed,
                });
            }
        }
    }

    // max_key_index 必须与 key_count 匹配，确保 Y 轴显示完整
    let max_key_index = (KEY_COUNT.saturating_sub(1)) as f32;

    // canvas_size 必须设置为视频帧尺寸，否则 scissor rect 会被默认 800x600 裁剪
    let canvas_size = (w, h);

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (w, h),
        scale_factor: 1.0,
        scroll: (scroll_x, scroll_y),
        zoom: (zoom_x, zoom_y),
        keyboard_width,
        ruler_height,
        note_instances,
        grid_instances,
        ruler_instances,
        keyboard_instances,
        ppq: ppq as f32,
        max_key_index,
        canvas_size,
        ..Default::default()
    }
}

// ── HiRes 贴图生成 ──

/// 将 MidiDocument 音符转换为洋葱皮生成所需格式
pub(super) fn convert_notes_to_onion_skin(
    document: &lumino_midi_loader::MidiDocument,
) -> Vec<Vec<OnionSkinNote>> {
    document
        .notes
        .iter()
        .enumerate()
        .map(|(track_idx, notes)| {
            let color = ARRANGEMENT_PALETTE[track_idx % ARRANGEMENT_PALETTE.len()];
            notes
                .iter()
                .map(|n| {
                    let color_u8 = [
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                        255,
                    ];
                    OnionSkinNote::from_note_event(n, color_u8)
                })
                .collect()
        })
        .collect()
}

/// 构建视频导出用 HiRes 配置
///
/// 视频导出强制启用贴图（enabled=true），并锁定为 Stretch 模式以匹配
/// 视频视口的任意缩放。
pub(super) fn build_hires_config_for_video(
    ui_config: &lumino_core::storage::config::UiConfig,
) -> HiResConfig {
    HiResConfig {
        enabled: true,
        measures_per_group: ui_config.hires_measures_per_group,
        tile_width_px: ui_config.hires_tile_width_px,
        cooldown_secs: ui_config.hires_cooldown_secs,
        gpu_mem_limit_mb: ui_config.hires_gpu_mem_limit_mb,
        render_mode: HiResRenderMode::Stretch,
        group_tile_mem_limit_mb: 256,
        cache_dir: HiResConfig::default().cache_dir,
    }
}

/// 为视频导出计算轻量 MIDI 哈希
pub(super) fn compute_video_midi_hash(document: &lumino_midi_loader::MidiDocument) -> String {
    let mut hash_input = Vec::new();
    hash_input.extend_from_slice(&document.total_ticks.to_le_bytes());
    hash_input.extend_from_slice(&(document.notes.len() as u32).to_le_bytes());
    for track in &document.notes {
        hash_input.extend_from_slice(&(track.len() as u32).to_le_bytes());
    }
    compute_midi_hash(&hash_input)
}

/// 生成视频导出用 HiRes 贴图
pub(super) fn generate_hires_video_tiles(
    document: &lumino_midi_loader::MidiDocument,
    config: &HiResConfig,
    ppq: u16,
    key_count: u16,
) -> std::collections::HashMap<lumino_gfx::TileCoord, lumino_gfx::GroupTile> {
    let mut notes = convert_notes_to_onion_skin(document);
    let midi_hash = compute_video_midi_hash(document);
    lumino_gfx::generate_all_tiles(
        &mut notes,
        config,
        ppq,
        key_count,
        document.total_ticks,
        &midi_hash,
        None,
    )
}

// ── 键盘贴图 ──

/// 生成完整键盘贴图（BGRA 像素数据，与视频帧格式一致）
///
/// 生成一个从最高键到最低键的完整键盘图像，与 note shader 的 Y 轴方向一致
/// （高键在上，低键在下）。返回 (pixels, width, height)。
///
/// 注意：key_count 固定为 128 以匹配标准 MIDI 键盘。
/// 使用 ceil() 进行像素到键位的映射，确保键盘边界与 note shader 对齐。
/// 直接生成 BGRA 格式以避免每帧合成时做 RGBA→BGRA 转换。
pub(super) fn generate_keyboard_texture(
    _width: u32,
    height: u32,
    key_count: u16,
) -> (Vec<u8>, u32, u32) {
    const KB_WIDTH: f32 = 60.0;
    const RULER_HEIGHT: f32 = 30.0;
    const KEY_COUNT: u16 = 128;
    let kb_w = KB_WIDTH as u32;

    // 键盘区域从 ruler 下方开始
    let ruler_h = RULER_HEIGHT as u32;
    if height <= ruler_h || key_count == 0 {
        return (Vec::new(), 0, 0);
    }
    let kb_h = height - ruler_h;
    let key_count_f = KEY_COUNT as f32;
    let zoom_y = kb_h as f32 / key_count_f;

    let mut pixels = vec![0u8; (kb_w * kb_h * 4) as usize];

    for py in 0..kb_h {
        // Y 向：键 0 在底部，最高键在顶部（与 note shader 一致）
        // 使用 ceil() 确保键盘边界与 note shader 的精确边界匹配：
        // note shader: screen_y = (max_key_index - key) * zoom_y + ruler_height
        // 键盘像素 py 映射到 key = ceil(max_key_index - py / zoom_y)
        // 这保证了每个键占据的像素范围与 note shader 渲染的矩形完全一致
        let key_f = (key_count_f - 1.0) - py as f32 / zoom_y;
        let key_idx = key_f.ceil() as i32;
        if key_idx < 0 || key_idx >= KEY_COUNT as i32 {
            continue;
        }
        let is_black = is_black_key(key_idx as isize);

        // 标准黑白色：白键纯白，黑键纯黑
        // 白键底部添加 1px 浅灰边框作键位分隔
        let is_bottom_border = {
            let next_key_f = (key_count_f - 1.0) - (py as f32 + 1.0) / zoom_y;
            let next_key_idx = next_key_f.ceil() as i32;
            next_key_idx != key_idx
        };

        for px in 0..kb_w {
            let idx = ((py * kb_w + px) * 4) as usize;

            let (r, g, b) = if is_black {
                // 黑键：纯黑，底部加 1px 深灰边框
                if is_bottom_border {
                    (40, 40, 40)
                } else {
                    (0, 0, 0) // 标准黑键
                }
            } else {
                // 白键：纯白，底部加 1px 浅灰边框
                if is_bottom_border {
                    (200, 200, 200)
                } else {
                    (255, 255, 255) // 标准白键
                }
            };

            pixels[idx] = b; // BGRA: B 通道
            pixels[idx + 1] = g; // BGRA: G 通道
            pixels[idx + 2] = r; // BGRA: R 通道
            pixels[idx + 3] = 255;
        }
    }

    (pixels, kb_w, kb_h)
}

// ── 标尺小节号数字渲染 ──

/// 5x7 位图字体：数字 0-9
///
/// 每个数字 5 列宽、7 行高，每行用一个 u8 位掩码表示（LSB = 左端像素）。
const DIGIT_BITMAPS: [[u8; 7]; 10] = [
    // 0
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ],
    // 1
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ],
    // 2
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ],
    // 3
    [
        0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110,
    ],
    // 4
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ],
    // 5
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ],
    // 6
    [
        0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ],
    // 7
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ],
    // 8
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ],
    // 9
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
    ],
];

/// 位图字体原始尺寸（列数）
const DIGIT_BITMAP_W: u32 = 5;
/// 位图字体原始尺寸（行数）
const DIGIT_BITMAP_H: u32 = 7;
/// 缩放倍数
const DIGIT_SCALE: u32 = 2;
/// 数字位图渲染后宽度（像素）
const DIGIT_W: u32 = DIGIT_BITMAP_W * DIGIT_SCALE;
/// 数字位图渲染后高度（像素）
const DIGIT_H: u32 = DIGIT_BITMAP_H * DIGIT_SCALE;
/// 数字间距（像素）
const DIGIT_SPACING: u32 = DIGIT_SCALE;

/// 在 BGRA 帧数据上绘制一个数字字符（2x 缩放）
///
/// 每个位图像素渲染为 `DIGIT_SCALE × DIGIT_SCALE` 的方块。
/// `x`、`y` 为左上角像素坐标。
/// `color` 为 BGRA 颜色值（[B, G, R, A]）。
fn draw_digit(frame: &mut [u8], frame_width: u32, digit: u8, x: u32, y: u32, color: [u8; 4]) {
    let Some(bitmap) = DIGIT_BITMAPS.get(digit as usize) else {
        return;
    };
    let frame_w = frame_width as usize;
    for row in 0..DIGIT_BITMAP_H {
        let mask = bitmap[row as usize];
        for col in 0..DIGIT_BITMAP_W {
            if mask & (1 << (DIGIT_BITMAP_W - 1 - col)) != 0 {
                // 每个位图像素扩展为 DIGIT_SCALE × DIGIT_SCALE 的方块
                for sy in 0..DIGIT_SCALE {
                    for sx in 0..DIGIT_SCALE {
                        let px = (x + col * DIGIT_SCALE + sx) as usize;
                        let py = (y + row * DIGIT_SCALE + sy) as usize;
                        let idx = (py * frame_w + px) * 4;
                        if idx + 3 < frame.len() {
                            frame[idx] = color[0]; // B
                            frame[idx + 1] = color[1]; // G
                            frame[idx + 2] = color[2]; // R
                            frame[idx + 3] = color[3]; // A
                        }
                    }
                }
            }
        }
    }
}

/// 在 BGRA 帧数据上绘制一个正整数
///
/// 数字渲染在 `(x, y)` 位置，各数字间有 `DIGIT_SPACING` 像素间距。
fn draw_number(
    frame: &mut [u8],
    frame_width: u32,
    number: u32,
    mut x: u32,
    y: u32,
    color: [u8; 4],
) {
    // 将数字转换为数字串
    let digits: Vec<u8> = if number == 0 {
        vec![0]
    } else {
        let mut n = number;
        let mut d = Vec::new();
        while n > 0 {
            d.push((n % 10) as u8);
            n /= 10;
        }
        d.reverse();
        d
    };

    for &digit in &digits {
        draw_digit(frame, frame_width, digit, x, y, color);
        x += DIGIT_W + DIGIT_SPACING;
    }
}

/// 将标尺小节号合成到视频帧上（BGRA 格式，in-place 修改）
pub(super) fn composite_ruler_numbers(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    scroll_x: f32,
    zoom_x: f32,
    keyboard_width: f32,
    ppq: u32,
) {
    if frame_width == 0 || frame_height == 0 || zoom_x <= 0.0 {
        return;
    }

    let ruler_h = 30u32;
    let ticks_per_measure = (ppq * 4) as f32;

    // 计算可见的小节范围
    let visible_tick_start = scroll_x / zoom_x;
    let visible_tick_end = (scroll_x + frame_width as f32) / zoom_x;

    let measure_start = (visible_tick_start / ticks_per_measure).floor() as u32;
    let measure_end = (visible_tick_end / ticks_per_measure).ceil() as u32;

    // 文本颜色：浅灰（BGRA 格式）
    let text_color: [u8; 4] = [220, 220, 220, 255];

    for measure in measure_start..=measure_end {
        let tick = measure as f32 * ticks_per_measure;
        let screen_x = keyboard_width + tick * zoom_x - scroll_x;

        if screen_x >= keyboard_width && screen_x <= frame_width as f32 {
            // 小节号从 1 开始
            let bar_number = measure + 1;
            // 文本位置：距离刻度线左侧 4px，距离顶部 4px
            let text_x = (screen_x as u32 + 4).min(frame_width.saturating_sub(1));
            let text_y = 4u32.min(ruler_h.saturating_sub(DIGIT_H));

            draw_number(frame, frame_width, bar_number, text_x, text_y, text_color);
        }
    }
}

// ── 键盘合成 ──

/// 将键盘贴图合成到视频帧上（BGRA 格式，in-place 修改）
///
/// 贴图与帧均为 BGRA 格式，直接逐行 memcpy，避免逐像素转换。
pub(super) fn composite_keyboard(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    keyboard_pixels: &[u8],
    kb_width: u32,
    kb_height: u32,
) {
    let ruler_h = 30u32;
    if frame_width == 0 || frame_height == 0 || keyboard_pixels.is_empty() {
        return;
    }
    let kb_w = kb_width.min(frame_width);
    let kb_h = kb_height.min(frame_height.saturating_sub(ruler_h));
    let row_bytes = (kb_w * 4) as usize;
    let frame_stride = (frame_width * 4) as usize;
    let kb_stride = (kb_width * 4) as usize;

    for py in 0..kb_h {
        let frame_y = ruler_h + py;
        if frame_y >= frame_height {
            break;
        }
        let frame_start = frame_y as usize * frame_stride;
        let kb_start = py as usize * kb_stride;
        if frame_start + row_bytes <= frame.len() && kb_start + row_bytes <= keyboard_pixels.len() {
            frame[frame_start..frame_start + row_bytes]
                .copy_from_slice(&keyboard_pixels[kb_start..kb_start + row_bytes]);
        }
    }
}
