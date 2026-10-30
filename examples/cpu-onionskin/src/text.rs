//! 文本绘制：复用仓库内置 NotoSansSC，支持中英文状态栏/标尺/键位标签。
//!
//! 只在 UI 线程低频调用（标尺小节号 + 键盘 C 标签 + 状态行），不是热路径；
//! 字形覆盖度按线性空间混合，与音符的 sRGB 口径保持一致。

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};

use crate::render::color::blend_pixel;

const FONT_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/fonts/NotoSansSC-Regular.ttf"
));

/// 文本渲染器（字体解析失败时退化为不绘制文本，不影响卷帘）。
pub struct TextRenderer {
    font: FontVec,
}

impl TextRenderer {
    /// 加载内置字体。
    #[must_use]
    pub fn new() -> Option<Self> {
        FontVec::try_from_vec(FONT_BYTES.to_vec())
            .ok()
            .map(|font| Self { font })
    }

    /// 文本宽度（像素）。
    #[must_use]
    pub fn width(&self, text: &str, size: f32) -> f32 {
        let scaled = self.font.as_scaled(PxScale::from(size));
        text.chars()
            .map(|ch| scaled.h_advance(scaled.glyph_id(ch)))
            .sum()
    }

    /// 在 RGBA/0RGB 像素缓冲上绘制文本。
    ///
    /// `x`/`y` 为文本左上角；越界像素自动裁剪。返回绘制宽度。
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        frame: &mut [u32],
        stride: usize,
        x: f32,
        y: f32,
        text: &str,
        color: u32,
        size: f32,
    ) -> f32 {
        let height = frame.len() / stride.max(1);
        let scaled = self.font.as_scaled(PxScale::from(size));
        let mut pen_x = x;
        for ch in text.chars() {
            if ch == ' ' {
                pen_x += scaled.h_advance(scaled.glyph_id(ch));
                continue;
            }
            let glyph_id = scaled.glyph_id(ch);
            if let Some(outline) = self.font.outline_glyph(glyph_id.with_scale_and_position(
                PxScale::from(size),
                ab_glyph::point(pen_x, y + scaled.ascent()),
            )) {
                let bounds = outline.px_bounds();
                outline.draw(|dx, dy, coverage| {
                    if coverage <= 0.0 {
                        return;
                    }
                    let px = bounds.min.x as i32 + dx as i32;
                    let py = bounds.min.y as i32 + dy as i32;
                    if px < 0 || py < 0 || px as usize >= stride || py as usize >= height {
                        return;
                    }
                    let index = py as usize * stride + px as usize;
                    frame[index] = blend_pixel(frame[index], color, coverage);
                });
            }
            pen_x += scaled.h_advance(glyph_id);
        }
        pen_x - x
    }
}

/// 画一个半透明底 + 文本的 HUD。
#[allow(clippy::too_many_arguments)]
pub fn draw_hud(
    frame: &mut [u32],
    stride: usize,
    x: f32,
    y: f32,
    text: &str,
    text_color: u32,
    background: u32,
    font: Option<&TextRenderer>,
) {
    let Some(font) = font else {
        return;
    };
    let size = 13.0;
    let width = font.width(text, size) + 12.0;
    let height = size + 8.0;
    let x = x.max(0.0);
    let y = y.max(0.0);
    fill_rect(
        frame,
        stride,
        x as i32,
        y as i32,
        (x + width) as i32,
        (y + height) as i32,
        background,
    );
    font.draw(frame, stride, x + 6.0, y + 3.0, text, text_color, size);
}

fn fill_rect(frame: &mut [u32], stride: usize, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
    let height = (frame.len() / stride.max(1)) as i32;
    for y in y0.max(0)..y1.min(height) {
        for x in x0.max(0)..x1.min(stride as i32) {
            frame[y as usize * stride + x as usize] = color;
        }
    }
}
