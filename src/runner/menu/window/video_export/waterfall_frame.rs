//! 视频导出瀑布流 CPU 帧渲染
//!
//! 包含 BGRA 帧绘制辅助函数与瀑布流帧渲染。
//! 位图字体（draw_digit / DIGIT_BITMAPS 等）也集中于此，
//! 并通过私有 use 在 video_export.rs 的标尺小节号合成中使用。

use lumino_core::palette::current_track_color_f32;
use lumino_gfx::is_black_key;
use lumino_midi_loader::MidiDocument;

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
pub(super) const DIGIT_BITMAP_W: u32 = 5;
/// 位图字体原始尺寸（行数）
pub(super) const DIGIT_BITMAP_H: u32 = 7;
/// 缩放倍数
pub(super) const DIGIT_SCALE: u32 = 2;
/// 数字位图渲染后宽度（像素）
pub(super) const DIGIT_W: u32 = DIGIT_BITMAP_W * DIGIT_SCALE;
/// 数字位图渲染后高度（像素）
pub(super) const DIGIT_H: u32 = DIGIT_BITMAP_H * DIGIT_SCALE;
/// 数字间距（像素）
pub(super) const DIGIT_SPACING: u32 = DIGIT_SCALE;

/// 在 BGRA 帧数据上绘制一个数字字符（2x 缩放）
///
/// 每个位图像素渲染为 `DIGIT_SCALE × DIGIT_SCALE` 的方块。
/// `x`、`y` 为左上角像素坐标。
/// `color` 为 BGRA 颜色值（[B, G, R, A]）。
pub(super) fn draw_digit(
    frame: &mut [u8],
    frame_width: u32,
    digit: u8,
    x: u32,
    y: u32,
    color: [u8; 4],
) {
    let Some(bitmap) = DIGIT_BITMAPS.get(digit as usize) else {
        return;
    };
    let frame_w = frame_width as usize;
    let row_bytes = frame_w * 4;
    let color_bytes = color;

    for row in 0..DIGIT_BITMAP_H {
        let mask = bitmap[row as usize];
        if mask == 0 {
            continue;
        }
        let base_row_start = ((y + row * DIGIT_SCALE) as usize) * row_bytes;

        for col in 0..DIGIT_BITMAP_W {
            if mask & (1 << (DIGIT_BITMAP_W - 1 - col)) == 0 {
                continue;
            }
            let block_x_bytes = ((x + col * DIGIT_SCALE) as usize) * 4;

            for sy in 0..DIGIT_SCALE {
                let row_start = base_row_start + (sy as usize) * row_bytes + block_x_bytes;
                let row_end = row_start + (DIGIT_SCALE as usize) * 4;
                if row_end <= frame.len() {
                    for px_offset in (0..DIGIT_SCALE as usize * 4).step_by(4) {
                        let dst = row_start + px_offset;
                        frame[dst..dst + 4].copy_from_slice(&color_bytes);
                    }
                }
            }
        }
    }
}

/// 将 BGRA 帧数据填充为黑色背景（使用 bulk fill + alpha 修复）
fn fill_bgra_black(frame: &mut [u8]) {
    frame.fill(0);
    for a in frame.iter_mut().skip(3).step_by(4) {
        *a = 255;
    }
}

/// 在 BGRA 帧上绘制一个填充矩形（使用批量行填充）
#[allow(clippy::too_many_arguments)]
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
    let start_byte = x as usize * 4;
    let pixel_count = (x_end - x) as usize;
    let pixel_bytes = pixel_count * 4;

    for py in y..y_end {
        let row_start = py as usize * row_bytes + start_byte;
        let row_end = row_start + pixel_bytes;
        if row_end > frame.len() {
            break;
        }
        frame[row_start..row_end].fill(color[0]);
        for ch in frame[row_start + 1..row_end].chunks_exact_mut(4) {
            ch[0] = color[1];
        }
        for ch in frame[row_start + 2..row_end].chunks_exact_mut(4) {
            ch[0] = color[2];
        }
        for ch in frame[row_start + 3..row_end].chunks_exact_mut(4) {
            ch[0] = color[3];
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
#[allow(clippy::too_many_arguments)]
pub fn render_waterfall_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    key_count: u16,
    waterfall_speed: f32,
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

    // 计算瀑布流的可见 tick 范围（速度越高，可见 tick 越少，滚动越快）
    let ticks_per_measure = ppq * 4;
    let speed = waterfall_speed.max(0.1);
    let visible_measure_count = (4.0f32 / speed).round().max(1.0) as u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1);
    let zoom_x = frame_width as f32 / key_count as f32;
    let zoom_y = content_height as f32 / viewport_tick_span as f32;

    // 当前 tick 在底部（键盘位置），未来音符从顶部下落
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span);

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
        let color_f = current_track_color_f32(note.track_idx as usize);
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
            let color_f = current_track_color_f32(note.track_idx as usize);
            let blue = (color_f[2] * 255.0).round() as u8;
            let green = (color_f[1] * 255.0).round() as u8;
            let red = (color_f[0] * 255.0).round() as u8;
            let note_key = note.key as usize;
            if note_key < 128 {
                active_key_colors[note_key] = Some([blue, green, red, 255]);
            }
        }
    }

    let kb_y = content_height;
    let black_kb_height = (kb_height as f64 * 0.6).round() as u32;
    let total_w = frame_width as f64;
    let white_key_count = (0..key_count)
        .filter(|&k| !is_black_key(k as isize))
        .count() as f64;
    let white_w = total_w / white_key_count;
    let black_w = white_w * 0.65;
    let black_w_offset = black_w * 0.5;

    // 预计算键盘布局：Vec<(x, w, is_black, key_index)>
    let mut kb_layout: Vec<(f32, f32, bool, u16)> = Vec::with_capacity(key_count as usize);
    let mut white_count = 0usize;
    for key in 0..key_count {
        if is_black_key(key as isize) {
            let boundary_x = white_count as f64 * white_w;
            let pos_x = (boundary_x - black_w_offset) as f32;
            kb_layout.push((pos_x, black_w as f32, true, key));
        } else {
            let pos_x = (white_count as f64 * white_w) as f32;
            kb_layout.push((pos_x, white_w as f32, false, key));
            white_count += 1;
        }
    }

    /// 混合活跃键颜色到基础色上（与 piano roll 的 OVERLAY_ALPHA 逻辑一致）
    fn blend_key_color(base: [u8; 4], overlay: Option<[u8; 4]>, alpha: u8) -> [u8; 4] {
        match overlay {
            Some(oc) if alpha > 0 => {
                let alpha_val = alpha as i32;
                [
                    (base[0] as i32 + (oc[0] as i32 - base[0] as i32) * alpha_val / 255)
                        .clamp(0, 255) as u8,
                    (base[1] as i32 + (oc[1] as i32 - base[1] as i32) * alpha_val / 255)
                        .clamp(0, 255) as u8,
                    (base[2] as i32 + (oc[2] as i32 - base[2] as i32) * alpha_val / 255)
                        .clamp(0, 255) as u8,
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
