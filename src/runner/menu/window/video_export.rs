//! 视频导出辅助函数
//!
//! 包含视频帧渲染参数构建、键盘贴图生成、标尺小节号合成等工具函数。

use lumino_gfx::{
    ARRANGEMENT_PALETTE, NoteInstance, RenderParams, generate_ruler_instances, pack_color,
};

pub mod cli_progress;
pub mod keyboard;
pub mod streaming;

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

    // 1. 网格线由 GPU 端 infinite_grid.wgsl 自动绘制，不再生成 CPU 实例
    let grid_instances = Vec::new();

    // 2. 生成标尺实例
    let ruler_instances =
        generate_ruler_instances(w, keyboard_width, ruler_height, scroll_x, zoom_x);

    // 3. 键盘使用 CPU 贴图方式（视频导出线程中 composite），GPU 不渲染键盘实例
    let keyboard_instances = Vec::new();

    // 4. 收集可见音符，按音轨分配颜色
    // 排序规则（与 yinhe 一致）：
    //   第一层：key 分行，不同 key 互不影响
    //   第二层：start_tick 升序，后 tick 在上（稳定排序下同 tick 的相对顺序保留）
    //   第三层：同 tick 同 key 时，track 索引大的在上（稳定排序自然保留插入顺序）
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);
    #[derive(Clone)]
    struct SortableNote {
        key: u8,
        start_tick: u32,
        length: u32,
        track_idx: u16,
    }
    let mut temp: Vec<SortableNote> = Vec::new();
    for (track_idx, notes) in document.notes.iter().enumerate() {
        for n in notes {
            if n.end_tick >= tick_start && n.start_tick <= tick_end {
                temp.push(SortableNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    length: n.length(),
                    track_idx: track_idx as u16,
                });
            }
        }
    }
    // 稳定排序：key → start_tick → track_idx（降序，后 track 在上）
    temp.sort_by_key(|n| (n.key, n.start_tick, u16::MAX - n.track_idx));
    let note_instances: Vec<NoteInstance> = temp
        .into_iter()
        .map(|n| {
            let color = ARRANGEMENT_PALETTE[n.track_idx as usize % ARRANGEMENT_PALETTE.len()];
            let color_packed = pack_color([color[0], color[1], color[2], 1.0]);
            NoteInstance {
                position: [n.start_tick as f32, n.key as f32],
                size_x: (n.length as f32).max(1.0),
                color_packed,
            }
        })
        .collect();

    // 首帧诊断：定位音符缺失问题
    static MEM_DIAG_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let diag_idx = MEM_DIAG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if diag_idx < 3 {
        let total_notes: usize = document.notes.iter().map(|v| v.len()).sum();
        tracing::info!(
            "内存模式诊断[{}]: note_instances={}, total_notes={}, tick={}, vis_range={}..{}",
            diag_idx,
            note_instances.len(),
            total_notes,
            tick,
            tick_start,
            tick_end,
        );
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

// ── 键盘贴图 ──

pub use keyboard::generate_keyboard_texture;

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
    let color_bytes = color; // [B, G, R, A]

    for row in 0..DIGIT_BITMAP_H {
        let mask = bitmap[row as usize];
        // 该行没有填充像素则跳过整行
        if mask == 0 {
            continue;
        }
        // base_row_start: 该位图行首字节在 frame 中的索引
        // py = y + row * DIGIT_SCALE
        let base_row_start = ((y + row * DIGIT_SCALE) as usize) * frame_w;

        for col in 0..DIGIT_BITMAP_W {
            if mask & (1 << (DIGIT_BITMAP_W - 1 - col)) == 0 {
                continue;
            }
            // 预计算方块左边缘在 frame 中的字节偏移
            // px = x + col * DIGIT_SCALE
            let block_x_bytes = ((x + col * DIGIT_SCALE) as usize) * 4;

            // 展开 DIGIT_SCALE x DIGIT_SCALE 的方块写入
            // 每行 DIGIT_SCALE 个像素，共 DIGIT_SCALE 行
            // 使用 copy_from_slice 替代逐像素写入
            for sy in 0..DIGIT_SCALE {
                let row_start = (base_row_start + (sy as usize) * frame_w) + block_x_bytes;
                let row_end = row_start + (DIGIT_SCALE as usize) * 4;
                if row_end <= frame.len() {
                    // 连续写入 DIGIT_SCALE 个像素
                    for px_offset in (0..DIGIT_SCALE as usize * 4).step_by(4) {
                        let dst = row_start + px_offset;
                        frame[dst..dst + 4].copy_from_slice(&color_bytes);
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
    // 使用固定大小数组避免 Vec 堆分配（最大支持 10 位数）
    let mut digits = [0u8; 10];
    let mut len = 0usize;

    if number == 0 {
        digits[0] = 0;
        len = 1;
    } else {
        let mut n = number;
        while n > 0 {
            digits[len] = (n % 10) as u8;
            len += 1;
            n /= 10;
        }
        digits[..len].reverse();
    }

    for &digit in &digits[..len] {
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

pub use keyboard::composite_keyboard;

// ── 瀑布流渲染 ──

/// 将 BGRA 帧数据填充为黑色背景（GPU 背景色不统一，统一设置为纯黑）
fn fill_bgra_black(frame: &mut [u8]) {
    for pixel in frame.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0, 0, 0, 255]);
    }
}

/// 在 BGRA 帧上绘制一个填充矩形
fn fill_bgra_rect(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: [u8; 4],
) {
    if w == 0 || h == 0 {
        return;
    }
    let x_end = (x + w).min(frame_width);
    let y_end = (y + h).min(frame_height);
    let row_bytes = frame_width as usize * 4;
    let pixel_bytes = (x_end - x) as usize * 4;

    for py in y..y_end {
        let row_start = py as usize * row_bytes + x as usize * 4;
        let row_end = row_start + pixel_bytes;
        if row_end > frame.len() {
            break;
        }
        // 逐像素填充该行
        for px in (row_start..row_end).step_by(4) {
            frame[px..px + 4].copy_from_slice(&color);
        }
    }
}

/// 在 BGRA 帧上以瀑布流风格渲染 MIDI 音符
///
/// 瀑布流风格：
/// - X 轴 = 音高（key），从左到右排列
/// - Y 轴 = 时间（tick），从上到下流动
/// - 音符渲染为水平彩色条，宽度固定为 1 个键宽，高度对应音符时长
/// - 当前播放时刻（tick）位于帧底部，历史音符向上延伸
/// - 键盘位于帧底部（钢琴风格：白键等分宽度，黑键 65% 宽 + 60% 高）
/// - 背景为纯黑色
pub fn render_waterfall_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &lumino_midi_loader::MidiDocument,
    tick: u32,
    ppq: u32,
    key_count: u16,
) {
    if frame_width == 0 || frame_height == 0 || key_count == 0 {
        return;
    }

    // 先填充纯黑背景
    fill_bgra_black(frame);

    // 键盘高度占帧高的 12%（与 nezha 默认键盘比例一致）
    let kb_height = (frame_height as f64 * 0.12).round() as u32;
    let kb_height = kb_height.max(20).min(frame_height / 3);
    let content_height = frame_height.saturating_sub(kb_height);
    if content_height == 0 {
        return;
    }

    // 计算瀑布流的可见 tick 范围
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = 4u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1);
    let zoom_x = frame_width as f32 / key_count as f32;
    let zoom_y = content_height as f32 / viewport_tick_span as f32;

    let tick_start = tick.saturating_sub(viewport_tick_span);
    let tick_end = tick;

    // 收集可见音符
    #[derive(Clone)]
    struct WaterfallNote {
        key: u8,
        start_tick: u32,
        end_tick: u32,
        track_idx: u16,
    }

    let mut notes: Vec<WaterfallNote> = Vec::new();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        for n in track_notes {
            if n.end_tick > tick_start && n.start_tick < tick_end {
                notes.push(WaterfallNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    end_tick: n.end_tick,
                    track_idx: track_idx as u16,
                });
            }
        }
    }

    // 渲染每个音符
    for note in &notes {
        let color_f = ARRANGEMENT_PALETTE[note.track_idx as usize % ARRANGEMENT_PALETTE.len()];
        let color: [u8; 4] = [
            (color_f[2] * 255.0).round() as u8,
            (color_f[1] * 255.0).round() as u8,
            (color_f[0] * 255.0).round() as u8,
            200,
        ];

        let note_x = (note.key as f32 * zoom_x).round() as u32;
        let note_w = zoom_x.ceil() as u32;

        let note_top = ((tick_end.saturating_sub(note.end_tick)) as f32 * zoom_y).round() as u32;
        let note_bottom =
            ((tick_end.saturating_sub(note.start_tick)) as f32 * zoom_y).round() as u32;
        let note_h = note_bottom.saturating_sub(note_top).max(1);

        fill_bgra_rect(
            frame,
            frame_width,
            content_height,
            note_x,
            note_top,
            note_w,
            note_h,
            color,
        );
    }

    // ── 钢琴风格键盘渲染（照抄 nezha 方案） ──
    //
    // 白键等分总宽度，黑键宽度为白键的 65%，位于相邻白键边界中间，
    // 黑键高度为键盘总高度的 60%，渲染顺序：白键（底层）→ 黑键（上层覆盖）。
    //
    // 活跃键高亮：收集当前 tick 下正在演奏的音符对应的键位颜色，
    // 按 60% 透明度混合到键位基础色上，与钢琴卷帘的演奏高亮效果一致。
    const OVERLAY_ALPHA: u8 = 153; // 60% 不透明度
    let mut active_key_colors: [Option<[u8; 4]>; 128] = [None; 128];
    for note in &notes {
        if note.start_tick <= tick && note.end_tick > tick {
            let color_f = ARRANGEMENT_PALETTE[note.track_idx as usize % ARRANGEMENT_PALETTE.len()];
            let b = (color_f[2] * 255.0).round() as u8;
            let g = (color_f[1] * 255.0).round() as u8;
            let r = (color_f[0] * 255.0).round() as u8;
            let key = note.key as usize;
            if key < 128 {
                active_key_colors[key] = Some([b, g, r, 255]);
            }
        }
    }

    let kb_y = content_height;
    let black_kb_height = (kb_height as f64 * 0.6).round() as u32;
    let total_w = frame_width as f64;
    let white_key_count = (0..key_count)
        .filter(|&k| !lumino_gfx::is_black_key(k as isize))
        .count() as f64;
    let white_w = total_w / white_key_count;
    let black_w = white_w * 0.65;
    let black_w_offset = black_w * 0.5;

    // 预计算键盘布局：Vec<(x, w, is_black, key_index)>
    let mut kb_layout: Vec<(f32, f32, bool, u16)> = Vec::with_capacity(key_count as usize);
    let mut white_count = 0usize;
    for key in 0..key_count {
        if lumino_gfx::is_black_key(key as isize) {
            let boundary_x = white_count as f64 * white_w;
            let x = (boundary_x - black_w_offset) as f32;
            kb_layout.push((x, black_w as f32, true, key));
        } else {
            let x = (white_count as f64 * white_w) as f32;
            kb_layout.push((x, white_w as f32, false, key));
            white_count += 1;
        }
    }

    /// 混合活跃键颜色到基础色上（与 piano roll 的 OVERLAY_ALPHA 逻辑一致）
    fn blend_key_color(base: [u8; 4], overlay: Option<[u8; 4]>, alpha: u8) -> [u8; 4] {
        match overlay {
            Some(oc) if alpha > 0 => {
                let a = alpha as i32;
                [
                    (base[0] as i32 + (oc[0] as i32 - base[0] as i32) * a / 255).clamp(0, 255)
                        as u8,
                    (base[1] as i32 + (oc[1] as i32 - base[1] as i32) * a / 255).clamp(0, 255)
                        as u8,
                    (base[2] as i32 + (oc[2] as i32 - base[2] as i32) * a / 255).clamp(0, 255)
                        as u8,
                    255,
                ]
            }
            _ => base,
        }
    }

    // 第一遍：白键（底层）
    for &(kx, kw, is_black, key) in &kb_layout {
        if is_black {
            continue;
        }
        let kx_i = kx.round() as u32;
        let kw_i = kw.ceil() as u32;
        let color = blend_key_color(
            [235, 235, 235, 255],
            active_key_colors[key as usize],
            OVERLAY_ALPHA,
        );
        fill_bgra_rect(
            frame,
            frame_width,
            frame_height,
            kx_i,
            kb_y,
            kw_i,
            kb_height,
            color,
        );
    }

    // 第二遍：黑键（上层覆盖，高度为键盘的 60%）
    for &(kx, kw, is_black, key) in &kb_layout {
        if !is_black {
            continue;
        }
        let kx_i = kx.round() as u32;
        let kw_i = kw.ceil() as u32;
        let color = blend_key_color(
            [41, 41, 42, 255],
            active_key_colors[key as usize],
            OVERLAY_ALPHA,
        );
        fill_bgra_rect(
            frame,
            frame_width,
            frame_height,
            kx_i,
            kb_y,
            kw_i,
            black_kb_height,
            color,
        );
    }
}
