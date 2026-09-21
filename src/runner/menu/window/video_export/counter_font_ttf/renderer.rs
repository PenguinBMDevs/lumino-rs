use std::collections::HashMap;

use ab_glyph::{Font, Point, PxScale, ScaleFont};
use lumino_message::events::window::video::CounterFont;

use super::super::counter_font_ttf_load::load_font;
use super::blend::blend_pixel;
use super::{DrawLineScaledInput, GlyphCacheEntry, TtfFontRenderer};

impl TtfFontRenderer {
    /// 创建渲染器。字号为像素高度（em 尺寸）。
    pub(crate) fn new(font: &CounterFont, font_size: u32) -> Result<Self, String> {
        let font = load_font(font)?;
        let px = (font_size.max(1) as f32).min(1024.0);
        let scaled = font.as_scaled(PxScale::from(px));
        let line_height = scaled.height().max(1.0).ceil() as u32 + 1;
        Ok(Self {
            font,
            px,
            line_height,
            cache: HashMap::new(),
        })
    }

    /// 行高（像素）
    pub(crate) fn line_height(&self) -> u32 {
        self.line_height
    }

    /// 确保字符已光栅化并缓存（不返回引用，避免借用冲突）。
    fn ensure_glyph(&mut self, ch: char) {
        if self.cache.contains_key(&ch) {
            return;
        }
        let glyph_id = self.font.glyph_id(ch);
        if glyph_id.0 == 0 {
            // 字体不含该字符：按空格处理（推进半个全角宽）
            let advance = (self.px * 0.5).round() as u32;
            let entry = GlyphCacheEntry {
                advance,
                width: 0,
                height: 0,
                offset_x: 0,
                offset_y: 0,
                alpha: Vec::new(),
            };
            self.cache.insert(ch, entry);
            return;
        }

        let scaled = self.font.as_scaled(PxScale::from(self.px));
        let glyph = glyph_id.with_scale_and_position(PxScale::from(self.px), Point::default());
        let outline = match self.font.outline_glyph(glyph) {
            Some(o) => o,
            None => {
                // 无轮廓（如空格）：只记推进宽度
                let advance = scaled.h_advance(glyph_id).round().max(0.0) as u32;
                let entry = GlyphCacheEntry {
                    advance,
                    width: 0,
                    height: 0,
                    offset_x: 0,
                    offset_y: 0,
                    alpha: Vec::new(),
                };
                self.cache.insert(ch, entry);
                return;
            }
        };

        let bounds = outline.px_bounds();
        let w = bounds.width().ceil() as u32;
        let h = bounds.height().ceil() as u32;
        let mut alpha = if w == 0 || h == 0 {
            Vec::new()
        } else {
            vec![0u8; (w * h) as usize]
        };
        if !alpha.is_empty() {
            outline.draw(|x, y, a| {
                let xi = x as usize;
                let yi = y as usize;
                if xi < w as usize && yi < h as usize {
                    let idx = yi * w as usize + xi;
                    if idx < alpha.len() {
                        // 累积混合（部分轮廓绘制可能重叠）
                        let cur = alpha[idx] as u32;
                        let na = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
                        alpha[idx] = (cur + na).min(255) as u8;
                    }
                }
            });
        }

        let entry = GlyphCacheEntry {
            advance: scaled.h_advance(glyph_id).round().max(0.0) as u32,
            width: w,
            height: h,
            offset_x: bounds.min.x.round() as i32,
            // 行顶 + ascent 为基线，位图相对基线再偏移 bounds.min.y
            offset_y: (scaled.ascent().round() as i32) + bounds.min.y.round() as i32,
            alpha,
        };
        self.cache.insert(ch, entry);
    }

    /// 查询字符缓存（`ensure_glyph` 之后调用）。
    fn glyph(&self, ch: char) -> Option<&GlyphCacheEntry> {
        self.cache.get(&ch)
    }

    /// 测量单行文本的像素宽度（含字形推进）。
    pub(crate) fn measure_line(&mut self, line: &str) -> u32 {
        let mut width = 0u32;
        for ch in line.chars() {
            self.ensure_glyph(ch);
            if let Some(g) = self.glyph(ch) {
                width = width.saturating_add(g.advance);
            }
        }
        width
    }

    /// 在 BGRA 帧上绘制单行文本，返回绘制宽度。
    ///
    /// `x`、`y` 为行左上角；逐字符 blit 缓存位图（alpha 混合到背景）。
    pub(crate) fn draw_line(
        &mut self,
        frame: &mut [u8],
        frame_width: u32,
        line: &str,
        x: u32,
        y: u32,
        color: [u8; 4],
    ) -> u32 {
        let frame_w = frame_width as usize;
        let row_bytes = frame_w * 4;
        let frame_len = frame.len();
        let mut cur_x = x as i64;

        for ch in line.chars() {
            self.ensure_glyph(ch);
            let Some(g) = self.glyph(ch) else { continue };
            let dst_x = cur_x + g.offset_x as i64;
            let dst_y = y as i64 + g.offset_y as i64;
            let (w, h) = (g.width as i64, g.height as i64);
            if g.width > 0 && g.height > 0 {
                for row in 0..h {
                    let fy = dst_y + row;
                    if fy < 0 || fy >= frame_len as i64 / row_bytes as i64 {
                        continue;
                    }
                    let row_start = (fy as usize) * row_bytes;
                    // 注意：必须用一段切片 [a..b]，两段 [a..][b..] 会在第二次切片时越界
                    let row_off = (row as usize) * g.width as usize;
                    let alpha_row = &g.alpha[row_off..row_off + g.width as usize];
                    for col in 0..w {
                        let fx = dst_x + col;
                        if fx < 0 || fx >= frame_w as i64 {
                            continue;
                        }
                        let a = alpha_row[col as usize] as u32;
                        if a == 0 {
                            continue;
                        }
                        let px = row_start + (fx as usize) * 4;
                        if px + 4 <= frame_len {
                            blend_pixel(&mut frame[px..px + 4], color, a);
                        }
                    }
                }
            }
            cur_x += g.advance as i64;
        }
        cur_x.saturating_sub(x as i64).min(u32::MAX as i64) as u32
    }

    /// 绘制单行文本（带最近邻放大倍率），返回绘制宽度。
    ///
    /// 数据曲线模式里程碑刻度文字放大用：每个光栅化像素绘制为
    /// `extra_scale × extra_scale` 方块（与点阵后端的整倍放大语义一致）。
    pub(crate) fn draw_line_scaled(&mut self, input: DrawLineScaledInput<'_>) -> u32 {
        let DrawLineScaledInput {
            frame,
            frame_width,
            line,
            x,
            y,
            color,
            extra_scale,
        } = input;
        let extra = extra_scale.max(1) as i64;
        let frame_w = frame_width as usize;
        let row_bytes = frame_w * 4;
        let frame_len = frame.len();
        let mut cur_x = x as i64;

        for ch in line.chars() {
            self.ensure_glyph(ch);
            let Some(g) = self.glyph(ch) else { continue };
            let dst_x = cur_x + g.offset_x as i64;
            let dst_y = y as i64 + g.offset_y as i64;
            let (w, h) = (g.width as i64, g.height as i64);
            if g.width > 0 && g.height > 0 {
                for row in 0..h {
                    let fy = dst_y + row * extra;
                    if fy < 0 || fy >= frame_len as i64 / row_bytes as i64 {
                        continue;
                    }
                    let row_off = (row as usize) * g.width as usize;
                    let alpha_row = &g.alpha[row_off..row_off + g.width as usize];
                    for col in 0..w {
                        let a = alpha_row[col as usize] as u32;
                        if a == 0 {
                            continue;
                        }
                        // 最近邻放大：alpha 像素扩展为 extra×extra 方块
                        for sy in 0..extra {
                            let fx_y = fy + sy;
                            if fx_y >= frame_len as i64 / row_bytes as i64 {
                                break;
                            }
                            let px_row = (fx_y as usize) * row_bytes;
                            for sx in 0..extra {
                                let fx = dst_x + col * extra + sx;
                                if fx < 0 || fx >= frame_w as i64 {
                                    continue;
                                }
                                let px = px_row + (fx as usize) * 4;
                                if px + 4 <= frame_len {
                                    blend_pixel(&mut frame[px..px + 4], color, a);
                                }
                            }
                        }
                    }
                }
            }
            cur_x += g.advance as i64 * extra;
        }
        cur_x.saturating_sub(x as i64).min(u32::MAX as i64) as u32
    }
}
