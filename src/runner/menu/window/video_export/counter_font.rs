//! 计数器模式字体渲染（统一入口）
//!
//! 双后端：
//! - **内置 5x7 点阵**（`Bitmap`）：零依赖，仅支持 ASCII（参考 Zenith GLTextEngine 与
//!   fmr GDI+ DrawString 的文本渲染思路，直接写 BGRA 像素）。
//! - **TTF/OTF 光栅化**（`Ttf`，见 `counter_font_ttf.rs`）：任意 Unicode（含中文），
//!   用于系统字体与自定义字体文件。
//!
//! 覆盖 ASCII 0x20-0x7E 全部 95 个可打印字符。字体数据表在 `counter_font_data.rs`。

use lumino_message::events::window::video::CounterFont;

use super::counter_font_data::FONT5X7;
use super::counter_font_ttf::TtfFontRenderer;

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

/// 字体渲染后端。
pub(super) enum FontBackend {
    /// 内置 5x7 点阵（仅 ASCII）
    Bitmap {
        /// 位图缩放倍数（>=1）
        scale: u32,
    },
    /// TTF/OTF 光栅化（任意 Unicode）
    Ttf(TtfFontRenderer),
}

/// 计数器文本渲染器。
///
/// 字号在构造时固定；TTF 后端内置 glyph 缓存，重复字符零光栅化开销。
pub(crate) struct CounterFontRenderer {
    backend: FontBackend,
    /// 实际使用的来源（诊断用）
    source: CounterFont,
    /// 字号（像素）
    font_size: u32,
}

impl CounterFontRenderer {
    /// 创建渲染器。`font` 为来源配置，`font_size` 为像素高度。
    ///
    /// 失败（字体文件不存在/解析失败）返回错误信息——调用方决定回退策略。
    pub(crate) fn new(font: &CounterFont, font_size: u32) -> Result<Self, String> {
        let font_size = font_size.max(1);
        match font {
            CounterFont::Bitmap => Ok(Self {
                backend: FontBackend::Bitmap {
                    scale: bitmap_scale(font_size),
                },
                source: CounterFont::Bitmap,
                font_size,
            }),
            CounterFont::System { .. } | CounterFont::File { .. } => {
                let ttf = TtfFontRenderer::new(font, font_size)?;
                Ok(Self {
                    backend: FontBackend::Ttf(ttf),
                    source: font.clone(),
                    font_size,
                })
            }
        }
    }

    /// 行高（像素）
    pub(crate) fn line_height(&self) -> u32 {
        match &self.backend {
            FontBackend::Bitmap { scale } => (CHAR_BITMAP_H + CHAR_ROW_SPACING) * scale,
            FontBackend::Ttf(ttf) => ttf.line_height(),
        }
    }

    /// 绘制单行文本，返回绘制宽度。
    pub(crate) fn draw_line(
        &mut self,
        frame: &mut [u8],
        frame_width: u32,
        line: &str,
        x: u32,
        y: u32,
        color: [u8; 4],
    ) -> u32 {
        match &mut self.backend {
            FontBackend::Bitmap { scale } => {
                let scale = *scale;
                let mut cur_x = x;
                for ch in line.chars() {
                    // 非 ASCII 字符点阵不支持：按空格宽度推进（不 panic）
                    if ch.is_ascii() {
                        draw_char(frame, frame_width, ch as u8, cur_x, y, scale, color);
                    }
                    cur_x = cur_x.saturating_add((CHAR_BITMAP_W + CHAR_SPACING) * scale);
                }
                cur_x.saturating_sub(x)
            }
            FontBackend::Ttf(ttf) => ttf.draw_line(frame, frame_width, line, x, y, color),
        }
    }

    /// 绘制单行文本（带额外放大倍率），返回绘制宽度。
    ///
    /// 数据曲线模式里程碑刻度（1k/10k/100k…）文字放大用；
    /// 点阵后端将放大倍率乘入位图 scale，TTF 后端做最近邻放大。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_line_scaled(
        &mut self,
        frame: &mut [u8],
        frame_width: u32,
        line: &str,
        x: u32,
        y: u32,
        color: [u8; 4],
        extra_scale: u32,
    ) -> u32 {
        let extra = extra_scale.max(1);
        match &mut self.backend {
            FontBackend::Bitmap { scale } => {
                let scale = (*scale).max(1) * extra;
                let mut cur_x = x;
                for ch in line.chars() {
                    // 非 ASCII 字符点阵不支持：按空格宽度推进（不 panic）
                    if ch.is_ascii() {
                        draw_char(frame, frame_width, ch as u8, cur_x, y, scale, color);
                    }
                    cur_x = cur_x.saturating_add((CHAR_BITMAP_W + CHAR_SPACING) * scale);
                }
                cur_x.saturating_sub(x)
            }
            FontBackend::Ttf(ttf) => {
                ttf.draw_line_scaled(frame, frame_width, line, x, y, color, extra)
            }
        }
    }

    /// 测量单行文本的像素宽度。
    pub(crate) fn measure_line(&mut self, line: &str) -> u32 {
        match &mut self.backend {
            FontBackend::Bitmap { scale } => bitmap_measure(line, *scale),
            FontBackend::Ttf(ttf) => ttf.measure_line(line),
        }
    }

    /// 诊断描述（首帧日志用）
    pub(crate) fn describe(&self) -> String {
        match &self.source {
            CounterFont::Bitmap => format!(
                "内置点阵 5x7，字号 {}px（scale {}）",
                self.font_size,
                self.font_size / CHAR_BITMAP_H
            ),
            CounterFont::System { family } => {
                format!("系统字体「{family}」，字号 {}px", self.font_size)
            }
            CounterFont::File { path } => {
                format!("自定义字体 {path}，字号 {}px", self.font_size)
            }
        }
    }
}

/// 字号 → 位图缩放倍数（7px 基础行高）。
fn bitmap_scale(font_size: u32) -> u32 {
    (font_size / CHAR_BITMAP_H).max(1)
}

/// 点阵测量：按字符数（含间距），非 ASCII 按空格宽度计。
fn bitmap_measure(line: &str, scale: u32) -> u32 {
    if scale == 0 {
        return 0;
    }
    let chars = line.chars().count();
    if chars == 0 {
        return 0;
    }
    chars as u32 * (CHAR_BITMAP_W + CHAR_SPACING) * scale - CHAR_SPACING * scale
}

/// 在 BGRA 帧上绘制一个字符（整数倍缩放）。
///
/// `x`、`y` 为左上角像素坐标（缩放后），`scale` 为位图缩放倍数（>=1），
/// `color` 为 BGRA 颜色值（[B, G, R, A]）。
fn draw_char(
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
        let mut r = CounterFontRenderer::new(&CounterFont::Bitmap, 7).expect("点阵渲染器");
        r.draw_line(&mut frame, 10, "1", 0, 0, color);
        // (2, 0) 处应有像素（行 0 × 宽 10 + 列 2）
        let idx = 2 * 4;
        assert_eq!(&frame[idx..idx + 4], &color);
        // (0, 0) 处应无像素
        let idx0 = 0;
        assert_eq!(&frame[idx0..idx0 + 4], &[0, 0, 0, 0]);
    }

    /// 缩放绘制：scale=2（字号 14）时像素块为 2x2
    #[test]
    fn test_draw_char_scale() {
        let mut frame = vec![0u8; 20 * 20 * 4];
        let color = [255, 255, 255, 255];
        let mut r = CounterFontRenderer::new(&CounterFont::Bitmap, 14).expect("点阵渲染器");
        r.draw_line(&mut frame, 20, "1", 0, 0, color);
        // 字符 '1' 第一行 bit3(0b00100 → col 2)，scale=2 → x 4..6, y 0..2
        for y in 0..2 {
            for x in 4..6 {
                let idx = (y * 20 + x) * 4;
                assert_eq!(&frame[idx..idx + 4], &color, "({x},{y}) 应有像素");
            }
        }
        // 空白处无像素（原点应为空）
        let idx = 0;
        assert_eq!(&frame[idx..idx + 4], &[0, 0, 0, 0]);
    }

    /// 换行绘制：两行文本的第二行应从第一行下方开始（行高 7+1=8）
    #[test]
    fn test_draw_line_positions() {
        let mut frame = vec![0u8; 64 * 32 * 4];
        let color = [255, 255, 255, 255];
        let mut r = CounterFontRenderer::new(&CounterFont::Bitmap, 7).expect("点阵渲染器");
        r.draw_line(&mut frame, 64, "1", 0, 0, color);
        r.draw_line(&mut frame, 64, "2", 0, 8, color);
        // 第一行 '1' 在 (2,0)（行 0 × 宽 64 + 列 2）
        let idx = 2 * 4;
        assert_eq!(&frame[idx..idx + 4], &color, "第一行 '1' 应有像素");
        // 第二行 '2' 第一行 0b01110 → bit3..bit1 → col1..3 有像素
        let idx2 = (8 * 64 + 1) * 4;
        assert_eq!(&frame[idx2..idx2 + 4], &color, "第二行 '2' 应有像素");
        // 第二行 '2' 左端 col0 无像素（行 8 × 宽 64）
        let idx3 = (8 * 64) * 4;
        assert_eq!(
            &frame[idx3..idx3 + 4],
            &[0, 0, 0, 0],
            "第二行 '2' 左端无像素"
        );
    }

    /// 单行文本测量
    #[test]
    fn test_measure_line() {
        let mut r = CounterFontRenderer::new(&CounterFont::Bitmap, 7).expect("点阵渲染器");
        // 单字符：5 + 1 间距 - 1 = 5
        assert_eq!(r.measure_line("1"), 5);
        // 两字符："11" → 5 + 1 + 5 = 11
        assert_eq!(r.measure_line("11"), 11);
        // 空行
        assert_eq!(r.measure_line(""), 0);
        // 缩放（字号 21 → scale 3）
        let mut r3 = CounterFontRenderer::new(&CounterFont::Bitmap, 21).expect("点阵渲染器");
        assert_eq!(r3.measure_line("1"), 15);
    }

    /// 越界绘制不应 panic（x/y 超出帧边界）
    #[test]
    fn test_draw_char_out_of_bounds_no_panic() {
        let mut frame = vec![0u8; 16 * 16 * 4];
        let color = [255, 255, 255, 255];
        let mut r = CounterFontRenderer::new(&CounterFont::Bitmap, 28).expect("点阵渲染器");
        r.draw_line(&mut frame, 16, "A", 100, 100, color);
        r.draw_line(&mut frame, 16, "A", u32::MAX, 0, color);
        r.draw_line(&mut frame, 16, "A", 0, u32::MAX, color);
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

    /// 点阵模式遇中文：不 panic，宽度按空格推进
    #[test]
    fn test_bitmap_chinese_no_panic() {
        let mut frame = vec![0u8; 64 * 32 * 4];
        let color = [255, 255, 255, 255];
        let mut r = CounterFontRenderer::new(&CounterFont::Bitmap, 7).expect("点阵渲染器");
        // draw_line 返回光标推进距离（含尾部间距）：3 字符 × 6 = 18
        let w = r.draw_line(&mut frame, 64, "音符1", 0, 0, color);
        assert_eq!(w, 18);
        // 测量宽度（不含尾部间距）：2 中文按空格宽 + 1 数字 = 2*6 + 5 = 17
        assert_eq!(r.measure_line("音符1"), 17);
        assert_eq!(r.measure_line("音符"), 11, "中文按空格宽度测量");
    }

    /// 字号为 0 → 强制 1px，不 panic
    #[test]
    fn test_font_size_zero_clamped() {
        let r = CounterFontRenderer::new(&CounterFont::Bitmap, 0).expect("点阵渲染器");
        assert!(r.line_height() >= 1);
    }
}
