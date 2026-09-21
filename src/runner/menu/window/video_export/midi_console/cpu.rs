use ab_glyph::{Font, Point, PxScale, ScaleFont};
use lumino_midi_loader::MidiDocument;

use super::font::load_monospace_font;
use super::*;

/// 播放速度（倍速）：每帧推进 1 个 tick，1x 速度下每秒推进 (ppq * bpm / 60) 个 tick
pub(super) fn play_speed(ppq: u32, bpm: f64, fps: u32) -> f64 {
    if bpm <= 0.0 {
        return 0.0;
    }
    (fps as f64) / (ppq as f64 * bpm / 60.0)
}

/// 逐通道控制面板字段格式化
pub(super) fn format_field(f: usize, v: i32, level: f32) -> (String, [u8; 3]) {
    // 高亮强度随 ctrl_level 从常态色平滑过渡到告警红（控制变化高亮淡出动画）
    let col = mix([205, 205, 185], WARN, level.clamp(0.0, 1.0));
    let s = match f {
        0 => {
            if v < 0 {
                "---".to_string()
            } else {
                format!("{:>3}", v + 1)
            }
        } // PC（program + 1）
        1 => format!("{:>3}", v), // VOL
        2 => format!("{:>3}", v), // EXP
        3 => format!("{:>3}", v), // PAN
        4 => {
            if v == i32::MIN {
                "----".to_string()
            } else {
                format!("{:>4}", v)
            }
        } // P.BEND
        5 => format!("{:>3}", v), // P.RANGE
        6 => format!("{:>3}", v), // MOD
        7 => format!("{:>3}", v), // HOLD
        8 => format!("{:>3}", v), // CUT
        9 => format!("{:>3}", v), // RESO
        10 => format!("{:>3}", v), // ATT
        11 => format!("{:>3}", v), // DEC
        12 => format!("{:>3}", v), // REL
        _ => "---".to_string(),
    };
    (s, col)
}

/// CC 控制器号 → 控制面板字段索引（用于高亮触发）
pub(super) fn cc_field_index(c: u8) -> Option<usize> {
    Some(match c {
        7 => 1,
        11 => 2,
        10 => 3,
        6 => 5,
        1 => 6,
        64 => 7,
        74 => 8,
        71 => 9,
        73 => 10,
        75 => 11,
        72 => 12,
        _ => return None,
    })
}

/// 逐通道按键（key%12 映射）是否为黑键
fn is_black_key(k: usize) -> bool {
    matches!(k % 12, 1 | 3 | 6 | 8 | 10)
}

/// 键颜色：底色（黑/白键区分）按亮度水平 level（0=熄灭，1=点亮）平滑过渡到暖色，
/// 因此按键的亮起与熄灭都是连续渐变动画。
pub(super) fn key_color(k: usize, level: f32, warm: [u8; 3]) -> [u8; 3] {
    let base: [u8; 3] = if is_black_key(k) {
        KEY_BLACK
    } else {
        KEY_WHITE
    };
    let t = level.clamp(0.0, 1.0);
    [
        lerp(base[0], warm[0], t),
        lerp(base[1], warm[1], t),
        lerp(base[2], warm[2], t),
    ]
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}

/// 两个 RGB 颜色按 t 线性混合
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

/// 向目标值以固定步长连续趋近（用于亮度过渡动画）
#[inline]
pub(super) fn ramp(cur: f32, tgt: f32, rate: f32) -> f32 {
    if cur < tgt {
        (cur + rate).min(tgt)
    } else if cur > tgt {
        (cur - rate).max(tgt)
    } else {
        cur
    }
}

#[inline]
pub(super) fn cell_idx(r: u32, c: u32) -> usize {
    (r * COLS + c) as usize
}

pub(super) fn set_cell(grid: &mut [Cell], r: u32, c: u32, ch: char, fg: [u8; 3], bg: [u8; 3]) {
    if r < ROWS && c < COLS {
        grid[cell_idx(r, c)] = Cell { ch, fg, bg };
    }
}

pub(super) fn set_text(grid: &mut [Cell], r: u32, c: u32, text: &str, fg: [u8; 3], bg: [u8; 3]) {
    for (i, ch) in text.chars().enumerate() {
        set_cell(grid, r, c + i as u32, ch, fg, bg);
    }
}

/// `render_midicomsole_frame` / `_gpu` 共用的帧渲染参数（8 参结构体化，满足 `too_many_arguments`）。
pub struct MidiConsoleFrameArgs<'a> {
    /// 状态化字符网格渲染器
    pub renderer: &'a mut MidiConsoleRenderer,
    /// 输出 BGRA 帧缓冲
    pub frame: &'a mut [u8],
    /// 帧宽（像素）
    pub frame_width: u32,
    /// 帧高（像素）
    pub frame_height: u32,
    /// 源 MIDI 文档
    pub document: &'a MidiDocument,
    /// 当前 tick
    pub tick: u32,
    /// 每四分音符 tick 数
    pub ppq: u32,
    /// 帧率
    pub fps: u32,
}

/// 把字符网格栅格化为 BGRA 帧（按 cell 比例，含半块字符字形）
pub fn render_midicomsole_frame(args: MidiConsoleFrameArgs<'_>) {
    let MidiConsoleFrameArgs {
        renderer,
        frame,
        frame_width,
        frame_height,
        document,
        tick,
        ppq,
        fps,
    } = args;
    let gw = COLS as usize;
    let gh = ROWS as usize;
    let fw = frame_width as usize;
    let fh = frame_height as usize;
    let out_len = fw * fh * 4;
    if frame.len() < out_len {
        return;
    }

    // 1) 渲染字符网格
    let mut grid = vec![Cell::blank(); gw * gh];
    renderer.render(&mut grid, document, tick, ppq, fps);

    let cell_w = frame_width as f32 / COLS as f32;
    let cell_h = frame_height as f32 / ROWS as f32;

    // 2) 栅格化（参照本仓 ui-editor 的 ab_glyph 用法：glyph_id + outline_glyph）
    if let Some(font) = load_monospace_font() {
        let scale = PxScale {
            x: cell_h,
            y: cell_h,
        };
        let scaled = font.as_scaled(scale);
        let ascent = scaled.ascent();
        let descent = scaled.descent();
        let glyph_h = ascent - descent;
        for r in 0..gh {
            for c in 0..gw {
                let cell = grid[r * gw + c];
                // 先填充背景
                fill_rect(
                    frame,
                    (fw, fh),
                    (c as f32 * cell_w, r as f32 * cell_h, cell_w, cell_h),
                    cell.bg,
                );
                if cell.ch != ' ' {
                    let gid = font.glyph_id(cell.ch);
                    let ha = scaled.h_advance(gid);
                    let off_x = (cell_w - ha) / 2.0;
                    let off_y = (cell_h - glyph_h) / 2.0;
                    let baseline_x = c as f32 * cell_w + off_x;
                    let baseline_y = r as f32 * cell_h + off_y + ascent;
                    let glyph = gid.with_scale_and_position(
                        scale,
                        Point {
                            x: baseline_x,
                            y: baseline_y,
                        },
                    );
                    if let Some(outline) = font.outline_glyph(glyph) {
                        // 关键：ab_glyph 的 draw 回调坐标是「相对字形包围盒左上角」，
                        // 必须叠加 px_bounds().min 才是帧内绝对像素（与本仓 text_tool 完全一致）
                        let b = outline.px_bounds();
                        outline.draw(|px, py, cov| {
                            let x = (px as f32 + b.min.x).round() as i32;
                            let y = (py as f32 + b.min.y).round() as i32;
                            if x < 0 || y < 0 {
                                return;
                            }
                            let di = (y as usize * fw + x as usize) * 4;
                            if di + 3 >= frame.len() {
                                return;
                            }
                            let a = cov.clamp(0.0, 1.0);
                            frame[di] = blend(cell.bg[0], cell.fg[0], a);
                            frame[di + 1] = blend(cell.bg[1], cell.fg[1], a);
                            frame[di + 2] = blend(cell.bg[2], cell.fg[2], a);
                            frame[di + 3] = 255;
                        });
                    }
                }
            }
        }
    } else {
        // 降级：无字体时把非空格当作前景色块填充（保证仍有可见输出）
        for r in 0..gh {
            for c in 0..gw {
                let cell = grid[r * gw + c];
                let col = if cell.ch == ' ' { cell.bg } else { cell.fg };
                fill_rect(
                    frame,
                    (fw, fh),
                    (c as f32 * cell_w, r as f32 * cell_h, cell_w, cell_h),
                    col,
                );
            }
        }
    }

    // 3) 复古终端 CRT 后处理：静态扫描线 + 随 tick 移动的高亮扫描带（动态发光）
    apply_crt_effect(frame, fw, fh, tick);
}

/// 背景色与前景色按覆盖率混合（BGRA 顺序）
#[inline]
fn blend(bg: u8, fg: u8, a: f32) -> u8 {
    (fg as f32 * a + bg as f32 * (1.0 - a)).clamp(0.0, 255.0) as u8
}

/// 填充矩形（BGRA），`size=(宽, 高)`，`rect=(x0, y0, 宽, 高)`（8 参结构体化，满足 `too_many_arguments`）。
fn fill_rect(frame: &mut [u8], size: (usize, usize), rect: (f32, f32, f32, f32), color: [u8; 3]) {
    let (fw, fh) = size;
    let (x0, y0, w, h) = rect;
    let x0 = x0.max(0.0) as i64;
    let y0 = y0.max(0.0) as i64;
    let x1 = ((x0 as f32 + w) as i64).min(fw as i64);
    let y1 = ((y0 as f32 + h) as i64).min(fh as i64);
    for y in y0..y1 {
        let row = y as usize * fw;
        for x in x0..x1 {
            let di = (row + x as usize) * 4;
            if di + 3 < frame.len() {
                frame[di] = color[2];
                frame[di + 1] = color[1];
                frame[di + 2] = color[0];
                frame[di + 3] = 255;
            }
        }
    }
}

/// 复古终端 CRT 后处理：在每个像素上叠加扫描线纹理与一条随时间（tick）向下移动的高亮扫描带，
/// 营造动态发光扫描线的复古显示器质感。扫描带位置由 tick 驱动，逐帧变化即产生「流动」动画。
pub(super) fn apply_crt_effect(frame: &mut [u8], fw: usize, fh: usize, tick: u32) {
    let scan_period: usize = 3; // 每 3 行一条扫描暗线
    let scan_dark: f32 = 0.82; // 扫描暗线压暗系数
    let band_speed: f32 = 6.0; // 高亮扫描带每 tick 下移像素数
    let band_center = (tick as f32 * band_speed) % fh as f32;
    let band_width: f32 = 26.0; // 高亮扫描带半宽（高斯）
    let band_strength: f32 = 0.28; // 高亮扫描带增益
    for y in 0..fh {
        // 扫描线：每隔 scan_period 行整体压暗
        let scan = if y % scan_period == 0 { scan_dark } else { 1.0 };
        // 移动高亮带：以 band_center 为中心的高斯发光
        let dy = (y as f32 - band_center).abs();
        let glow = (-(dy * dy) / (2.0 * band_width * band_width)).exp() * band_strength;
        let factor = scan * (1.0 + glow);
        let row = y * fw;
        for x in 0..fw {
            let di = (row + x) * 4;
            frame[di] = (frame[di] as f32 * factor).clamp(0.0, 255.0) as u8;
            frame[di + 1] = (frame[di + 1] as f32 * factor).clamp(0.0, 255.0) as u8;
            frame[di + 2] = (frame[di + 2] as f32 * factor).clamp(0.0, 255.0) as u8;
        }
    }
}
