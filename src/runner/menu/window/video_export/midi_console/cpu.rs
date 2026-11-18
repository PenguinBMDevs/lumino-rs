use ab_glyph::{Font, PxScale, ScaleFont};
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
    crate::profiling::tracy_zone!("app.mc.grid", {
        renderer.render(&mut grid, document, tick, ppq, fps);
    });

    let cell_w = frame_width as f32 / COLS as f32;
    let cell_h = frame_height as f32 / ROWS as f32;

    // 2) 栅格化（参照本仓 ui-editor 的 ab_glyph 用法：glyph_id + outline_glyph）
    crate::profiling::tracy_zone!("app.mc.raster", {
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
                        let ha = scaled.h_advance(font.glyph_id(cell.ch));
                        let off_x = (cell_w - ha) / 2.0;
                        let off_y = (cell_h - glyph_h) / 2.0;
                        let baseline_x = c as f32 * cell_w + off_x;
                        let baseline_y = r as f32 * cell_h + off_y + ascent;
                        // 覆盖率按（字符, 基线亚像素相位）缓存：每帧只做颜色混合，
                        // 不再重复展平曲线 + 扫描线栅格化（像素逐位等价，见 glyph_cache）
                        let cached = super::glyph_cache::cached_glyph(
                            &font, scale, cell.ch, baseline_x, baseline_y,
                        );
                        // 关键：ab_glyph 的覆盖率坐标相对「字形包围盒左上角」，
                        // 必须叠加包围盒 min 才是帧内绝对像素（与本仓 text_tool 一致）。
                        // 样本坐标已预取整，这里只做整数加法；颜色 u8→f32 提到循环外。
                        let b_min_x = cached.base_min.0 + baseline_x.trunc();
                        let b_min_y = cached.base_min.1 + baseline_y.trunc();
                        let (bx, by) = (b_min_x as i32, b_min_y as i32);
                        let fg_f = [cell.fg[0] as f32, cell.fg[1] as f32, cell.fg[2] as f32];
                        let bg_f = [cell.bg[0] as f32, cell.bg[1] as f32, cell.bg[2] as f32];
                        let fg_word = pack_bgra(cell.fg);
                        let bg_word = pack_bgra(cell.bg);
                        for &(ox, oy, cov) in &cached.samples {
                            let x = bx + ox;
                            let y = by + oy;
                            if x < 0 || y < 0 {
                                continue;
                            }
                            let di = (y as usize * fw + x as usize) * 4;
                            if di + 3 >= frame.len() {
                                continue;
                            }
                            let word = coverage_word(fg_f, bg_f, fg_word, bg_word, cov);
                            frame[di..di + 4].copy_from_slice(&word.to_le_bytes());
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
    });

    // 3) 复古终端 CRT 后处理：静态扫描线 + 随 tick 移动的高亮扫描带（动态发光）
    crate::profiling::tracy_zone!("app.mc.crt", {
        apply_crt_effect(frame, fw, fh, tick);
    });
}

/// 打包 BGRA 像素（alpha 恒 255，与导出编码器消费顺序一致）。
#[inline]
pub(super) fn pack_bgra(color: [u8; 3]) -> u32 {
    (color[0] as u32) | ((color[1] as u32) << 8) | ((color[2] as u32) << 16) | 0xFF00_0000
}

/// 覆盖率 → BGRA 像素。
///
/// 两端（0 / ≥1）直接返回常量：原式 `f*a + b*(1-a)` 在 a=0/1 时精确等于
/// b / f（`f*1+b*0`、`f*0+b*1`），省去浮点混合；中间仍用原式逐通道计算，
/// 与逐帧栅格化实现逐位一致。
#[inline]
pub(super) fn coverage_word(
    fg_f: [f32; 3],
    bg_f: [f32; 3],
    fg_word: u32,
    bg_word: u32,
    cov: f32,
) -> u32 {
    let a = cov.clamp(0.0, 1.0);
    if a >= 1.0 {
        return fg_word;
    }
    if a <= 0.0 {
        return bg_word;
    }
    let inv = 1.0 - a;
    let b = (fg_f[0] * a + bg_f[0] * inv).clamp(0.0, 255.0) as u32;
    let g = (fg_f[1] * a + bg_f[1] * inv).clamp(0.0, 255.0) as u32;
    let r = (fg_f[2] * a + bg_f[2] * inv).clamp(0.0, 255.0) as u32;
    b | (g << 8) | (r << 16) | 0xFF00_0000
}

/// 填充矩形（BGRA），`size=(宽, 高)`，`rect=(x0, y0, 宽, 高)`（8 参结构体化，满足 `too_many_arguments`）。
pub(super) fn fill_rect(
    frame: &mut [u8],
    size: (usize, usize),
    rect: (f32, f32, f32, f32),
    color: [u8; 3],
) {
    let (fw, fh) = size;
    let (x0, y0, w, h) = rect;
    let x0 = x0.max(0.0) as i64;
    let y0 = y0.max(0.0) as i64;
    let x1 = ((x0 as f32 + w) as i64).min(fw as i64);
    let y1 = ((y0 as f32 + h) as i64).min(fh as i64);
    if x1 <= x0 {
        return;
    }
    // 行内整段像素填充：输出与逐像素写完全一致（同一 BGRA 像素、同一裁剪边界）。
    // cell 宽度通常 10-20px（40-80B）：整行一次 memcpy 比逐像素 4 字节 store 快。
    let pixel = [color[2], color[1], color[0], 255];
    let width = (x1 - x0) as usize;
    let bytes = width * 4;
    if bytes <= 64 {
        let mut pattern = [0u8; 64];
        for px in pattern.as_chunks_mut::<4>().0 {
            *px = pixel;
        }
        for y in y0..y1 {
            let start = (y as usize * fw + x0 as usize) * 4;
            let end = start + bytes;
            if end > frame.len() {
                continue;
            }
            frame[start..end].copy_from_slice(&pattern[..bytes]);
        }
    } else {
        for y in y0..y1 {
            let start = (y as usize * fw + x0 as usize) * 4;
            let end = start + bytes;
            if end > frame.len() {
                continue;
            }
            for px in frame[start..end].as_chunks_mut::<4>().0 {
                *px = pixel;
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
    // 每行亮度系数相同，先建 256 项查表再整行映射：逐像素 3 次 f32 乘加 + 截断
    // 降到 3 次字节查表，输出与原地计算逐位一致（同式同截断）。
    let mut lut = [0u8; 256];
    for y in 0..fh {
        // 扫描线：每隔 scan_period 行整体压暗
        let scan = if y % scan_period == 0 { scan_dark } else { 1.0 };
        // 移动高亮带：以 band_center 为中心的高斯发光
        let dy = (y as f32 - band_center).abs();
        let glow = (-(dy * dy) / (2.0 * band_width * band_width)).exp() * band_strength;
        let factor = scan * (1.0 + glow);
        for (v, slot) in lut.iter_mut().enumerate() {
            *slot = (v as f32 * factor).clamp(0.0, 255.0) as u8;
        }
        // 整行按 u32 处理：1 次 4 字节读 + 3 次查表 + 1 次 4 字节写，
        // 替代逐像素 4 次字节读/写；alpha 原样保留（与旧实现一致）。
        let start = y * fw * 4;
        let end = start + fw * 4;
        for px in frame[start..end].as_chunks_mut::<4>().0 {
            let word = u32::from_le_bytes(*px);
            let b = lut[(word & 0xFF) as usize] as u32;
            let g = lut[((word >> 8) & 0xFF) as usize] as u32;
            let r = lut[((word >> 16) & 0xFF) as usize] as u32;
            let out = b | (g << 8) | (r << 16) | (word & 0xFF00_0000);
            *px = out.to_le_bytes();
        }
    }
}
