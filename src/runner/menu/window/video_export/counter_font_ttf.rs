//! 计数器模式 TTF/OTF 字体后端（ab_glyph 光栅化）
//!
//! 负责：glyph 光栅化缓存、将文本以任意 Unicode 字符（含中文）绘制到 BGRA 帧上。
//! 字体加载（系统路径表 / 文件解析）见 `counter_font_ttf_load.rs`。
//!
//! 与内置 5x7 点阵后端（`counter_font.rs`）互补：点阵仅支持 ASCII，
//! 本后端支持全部 Unicode 字符，用于中文字符模板渲染。

use std::collections::HashMap;

use ab_glyph::FontArc;

/// `draw_line_scaled` 入参（替代 7 个位置参数，消除 `too_many_arguments`）。
pub(crate) struct DrawLineScaledInput<'a> {
    pub frame: &'a mut [u8],
    pub frame_width: u32,
    pub line: &'a str,
    pub x: u32,
    pub y: u32,
    pub color: [u8; 4],
    pub extra_scale: u32,
}

/// 单个 glyph 的光栅化结果（缓存用）。
struct GlyphCacheEntry {
    /// 水平推进宽度（像素）
    advance: u32,
    /// 位图宽度
    width: u32,
    /// 位图高度
    height: u32,
    /// 位图左上角相对当前光标位置的偏移（x）
    offset_x: i32,
    /// 位图左上角相对行顶的偏移（y）
    offset_y: i32,
    /// alpha 灰度数据（width × height）
    alpha: Vec<u8>,
}

/// TTF 字体渲染器（字号固定，glyph 位图缓存）。
pub(super) struct TtfFontRenderer {
    font: FontArc,
    px: f32,
    /// 行高（像素）：ascent - descent + line_gap
    line_height: u32,
    /// glyph 光栅化缓存
    cache: HashMap<char, GlyphCacheEntry>,
}

mod blend;
mod renderer;

#[cfg(test)]
mod tests;
