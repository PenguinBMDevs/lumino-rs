//! 计数器模式位图字体（ASCII 5x7）
//!
//! 参考 Zenith-MIDI NoteCountRender（GLTextEngine）与 fmr NoteCounter（GDI+ DrawString）
//! 的文本渲染思路，但 lumino 的视频导出 CPU 路径直接操作 BGRA 像素数组，
//! 因此使用内嵌 5x7 位图字体（与 `waterfall_frame.rs` 的 `DIGIT_BITMAPS` 同约定：
//! 每行一个 u8 位掩码，bit4 = 左端像素）。
//!
//! 覆盖 ASCII 0x20-0x7E 全部 95 个可打印字符，足够渲染计数器模板
//! （`Notes: {nc} / {tn}` 等）。中文字符不支持，模板请使用 ASCII。
//! 字体数据表在 `counter_font_data.rs`（独立文件控制行数）。

use super::counter_font_data::FONT5X7;

/// 位图字体原始宽度（列数）
pub(super) const CHAR_BITMAP_W: u32 = 5;
/// 位图字体原始高度（行数）
pub(super) const CHAR_BITMAP_H: u32 = 7;
/// 字符间距（原始像素）
pub(super) const CHAR_SPACING: u32 = 1;
/// 行间距（原始像素）
pub(super) const CHAR_ROW_SPACING: u32 = 1;

/// 获取字符的位图（空格返回空位图；不可打印字符按空格处理）。
fn char_bitmap(ch: u8) -> [u8; 7] {
    match ch {
        0x20..=0x7E => FONT5X7[(ch - 0x20) as usize],
        _ => [0; 7],
    }
}

/// 在 BGRA 帧上绘制一个字符（整数倍缩放）。
///
/// `x`、`y` 为左上角像素坐标（缩放后），`scale` 为位图缩放倍数（>=1），
/// `color` 为 BGRA 颜色值（[B, G, R, A]）。
pub(super) fn draw_char(
    frame: &mut [u8],
    frame_width: u32,
    ch: u8,
    x: u32,
    y: u32,
    scale: u32,
    color: [u8; 4],
) {
    if scale == 0 {
        return;
    }
    let bitmap = char_bitmap(ch);
    let frame_w = frame_width as usize;
    let row_bytes = frame_w * 4;

    for row in 0..CHAR_BITMAP_H {
        let mask = bitmap[row as usize];
        if mask == 0 {
            continue;
        }
        // saturating：越界坐标不会溢出（debug 模式 u32 加法会 panic）
        let base_row_start = (y.saturating_add(row * scale) as usize) * row_bytes;
        for col in 0..CHAR_BITMAP_W {
            if mask & (1 << (CHAR_BITMAP_W - 1 - col)) == 0 {
                continue;
            }
            let block_x_bytes = (x.saturating_add(col * scale) as usize) * 4;
            for sy in 0..scale {
                let row_start = base_row_start + (sy as usize) * row_bytes + block_x_bytes;
                let row_end = row_start + (scale as usize) * 4;
                if row_end <= frame.len() {
                    for px_offset in (0..scale as usize * 4).step_by(4) {
                        let dst = row_start + px_offset;
                        frame[dst..dst + 4].copy_from_slice(&color);
                    }
                }
            }
        }
    }
}

/// 在 BGRA 帧上绘制单行文本（返回绘制宽度）。
pub(super) fn draw_line(
    frame: &mut [u8],
    frame_width: u32,
    line: &str,
    x: u32,
    y: u32,
    scale: u32,
    color: [u8; 4],
) -> u32 {
    let mut cur_x = x;
    for ch in line.bytes() {
        draw_char(frame, frame_width, ch, cur_x, y, scale, color);
        cur_x += (CHAR_BITMAP_W + CHAR_SPACING) * scale;
    }
    cur_x.saturating_sub(x)
}

/// 测量单行文本的像素宽度（含字符间距）。
pub(super) fn measure_line(line: &str, scale: u32) -> u32 {
    if scale == 0 {
        return 0;
    }
    let chars = line.chars().count();
    if chars == 0 {
        return 0;
    }
    chars as u32 * (CHAR_BITMAP_W + CHAR_SPACING) * scale - CHAR_SPACING * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 字体表完整性：95 个 ASCII 可打印字符全覆盖
    #[test]
    fn test_font_table_covers_all_printable_ascii() {
        assert_eq!(FONT5X7.len(), 95, "应覆盖 ASCII 0x20-0x7E 共 95 个字符");
        for ch in 0x20u8..=0x7E {
            let bitmap = char_bitmap(ch);
            // 至少有一个非零行（除空格外的所有字符）
            if ch != 0x20 {
                assert!(
                    bitmap.iter().any(|&row| row != 0),
                    "字符 0x{ch:02X} 位图为空"
                );
            }
        }
    }

    /// 位图掩码约定：bit4 为左端像素（与 DIGIT_BITMAPS 一致）
    #[test]
    fn test_bitmap_msb_is_left() {
        // '1' 的位图：第一行 0b00100 → 绘制后第 2 列有像素
        let mut frame = vec![0u8; 10 * 10 * 4];
        let color = [0, 0, 255, 255]; // BGRA 红
        draw_char(&mut frame, 10, b'1', 0, 0, 1, color);
        // (2, 0) 处应有像素
        let idx = (0 * 10 + 2) * 4;
        assert_eq!(&frame[idx..idx + 4], &color);
        // (0, 0) 处应无像素
        let idx0 = 0;
        assert_eq!(&frame[idx0..idx0 + 4], &[0, 0, 0, 0]);
    }

    /// 缩放绘制：scale=2 时像素块为 2x2
    #[test]
    fn test_draw_char_scale() {
        let mut frame = vec![0u8; 20 * 20 * 4];
        let color = [255, 255, 255, 255];
        draw_char(&mut frame, 20, b'1', 0, 0, 2, color);
        // 字符 '1' 第一行 bit3(0b00100 → col 2)，scale=2 → x 4..6, y 0..2
        for y in 0..2 {
            for x in 4..6 {
                let idx = (y * 20 + x) * 4;
                assert_eq!(&frame[idx..idx + 4], &color, "({x},{y}) 应有像素");
            }
        }
        // 空白处无像素
        let idx = (0 * 20 + 0) * 4;
        assert_eq!(&frame[idx..idx + 4], &[0, 0, 0, 0]);
    }

    /// 换行绘制：两行文本的第二行应从第一行下方开始
    #[test]
    fn test_draw_line_positions() {
        let mut frame = vec![0u8; 64 * 32 * 4];
        let color = [255, 255, 255, 255];
        // 逐行绘制：第一行 (0,0)，第二行 (0,8)（7 行 + 1 行距）
        draw_line(&mut frame, 64, "1", 0, 0, 1, color);
        draw_line(&mut frame, 64, "2", 0, 8, 1, color);
        // 第一行 '1' 在 (2,0)
        let idx = (0 * 64 + 2) * 4;
        assert_eq!(&frame[idx..idx + 4], &color, "第一行 '1' 应有像素");
        // 第二行 '2' 第一行 0b01110 → bit3..bit1 → col1..3 有像素
        let idx2 = (8 * 64 + 1) * 4;
        assert_eq!(&frame[idx2..idx2 + 4], &color, "第二行 '2' 应有像素");
        // 第二行 '2' 左端 col0 无像素
        let idx3 = (8 * 64 + 0) * 4;
        assert_eq!(
            &frame[idx3..idx3 + 4],
            &[0, 0, 0, 0],
            "第二行 '2' 左端无像素"
        );
    }

    /// 单行文本测量
    #[test]
    fn test_measure_line() {
        // 单字符：5 + 1 间距 - 1 = 5
        assert_eq!(measure_line("1", 1), 5);
        // 两字符："11" → 5 + 1 + 5 = 11
        assert_eq!(measure_line("11", 1), 11);
        // 空行
        assert_eq!(measure_line("", 1), 0);
        // 缩放
        assert_eq!(measure_line("1", 2), 10);
        assert_eq!(measure_line("A", 3), 15);
    }

    /// 越界绘制不应 panic（x/y 超出帧边界）
    #[test]
    fn test_draw_char_out_of_bounds_no_panic() {
        let mut frame = vec![0u8; 16 * 16 * 4];
        let color = [255, 255, 255, 255];
        draw_char(&mut frame, 16, b'A', 100, 100, 4, color);
        draw_char(&mut frame, 16, b'A', u32::MAX, 0, 4, color);
        draw_char(&mut frame, 16, b'A', 0, u32::MAX, 4, color);
    }

    /// 模板常用字符全部有非空位图（字母数字 + 常用符号；空格除外——空格本应为空）
    #[test]
    fn test_template_chars_available() {
        for ch in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789:/%,.-_()[]{}" {
            let bitmap = char_bitmap(*ch);
            assert!(
                bitmap.iter().any(|&row| row != 0),
                "模板字符 '{}' 位图为空",
                *ch as char
            );
        }
        // 空格应为空位图
        assert!(char_bitmap(b' ').iter().all(|&row| row == 0));
    }
}
