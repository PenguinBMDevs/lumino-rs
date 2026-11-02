//! 音符颜色 / sRGB 编码 / 每轨调色板缓存。
//!
//! 与 GPU `onion_note.wgsl` 逐项对齐（源码为准，不按注释猜）：
//! - 洋葱皮实例的 `border_width` 低 16 位固定为 1，着色器
//!   `horiz_margin = border_px / (width * 0.5)`，因此**每侧实际边框 = 2px**；
//! - 颜色为实例固化的 `current_track_color_f32(track)`；shader 输出线性值到
//!   sRGB render target，CPU 端必须做 linear → sRGB 编码才能与屏幕像素一致；
//! - 边框 = `color * 0.4`，在线性空间计算后再编码；
//! - 音符不透明：内部帧 alpha 固定 0xFF，输出 RGBA8 alpha = 255。

use lumino_extras::palette;

/// UI 侧固定边框字段值（低 16 位）；实际每侧边框 = 2 × 本值。
pub const NOTE_BORDER_WIDTH: u32 = 1;
/// 着色器里每侧边框的实际像素数。
pub const NOTE_BORDER_PX: f32 = NOTE_BORDER_WIDTH as f32 * 2.0;
/// 边框加深因子（线性空间）。
pub const BORDER_DARKEN_FACTOR: f32 = 0.4;
/// 主轨固定蓝（线性，与 `onion_note.wgsl::MAIN_TRACK_COLOR` 一致）。
///
/// 洋葱皮渲染不会使用它；保留并导出用于与 GPU 事实源做常量对照。
pub const MAIN_TRACK_LINEAR: [f32; 3] = [0.2, 0.55, 1.0];

/// 线性值 → sRGB 字节。
#[must_use]
pub fn srgb_encode(linear: f32) -> u8 {
    let value = linear.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// sRGB 字节 → 线性值。
#[must_use]
pub fn srgb_decode(value: u8) -> f32 {
    let normalized = value as f32 / 255.0;
    if normalized <= 0.040_45 {
        normalized / 12.92
    } else {
        ((normalized + 0.055) / 1.055).powf(2.4)
    }
}

/// 四个 u8 打包为内部 `0xAARRGGBB`。
#[must_use]
pub const fn pack_argb(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// 线性 RGB → 不透明 `0xAARRGGBB`。
#[must_use]
pub fn linear_to_argb(rgb: [f32; 3]) -> u32 {
    pack_argb(
        srgb_encode(rgb[0]),
        srgb_encode(rgb[1]),
        srgb_encode(rgb[2]),
        0xFF,
    )
}

/// 线性 RGB × 系数后编码为不透明 `0xAARRGGBB`。
#[must_use]
pub fn linear_scaled_argb(rgb: [f32; 3], factor: f32) -> u32 {
    linear_to_argb([rgb[0] * factor, rgb[1] * factor, rgb[2] * factor])
}

/// 内部 `0xAARRGGBB` → 输出 RGBA8。
#[must_use]
pub const fn argb_to_rgba(pixel: u32) -> [u8; 4] {
    [
        ((pixel >> 16) & 0xFF) as u8,
        ((pixel >> 8) & 0xFF) as u8,
        (pixel & 0xFF) as u8,
        (pixel >> 24) as u8,
    ]
}

/// 每轨颜色缓存（填充 / 边框，内部 `0xAARRGGBB`）。
#[derive(Debug, Clone)]
pub struct NoteStyle {
    fill: Vec<u32>,
    border: Vec<u32>,
}

impl NoteStyle {
    /// 按当前调色板构建 `track_count` 条音轨的颜色缓存。
    ///
    /// 与 GPU 洋葱皮流一致：第 `track` 轨取
    /// `current_track_color_f32(track)`，主轨固定蓝不在此处使用。
    /// 超出缓存范围的轨道在查询时按同一定义即时计算，避免文档轨数变化后漏画。
    #[must_use]
    pub fn new(track_count: usize) -> Self {
        let mut fill = Vec::with_capacity(track_count);
        let mut border = Vec::with_capacity(track_count);
        for track in 0..track_count {
            let linear = palette_linear(track);
            fill.push(linear_to_argb(linear));
            border.push(linear_scaled_argb(linear, BORDER_DARKEN_FACTOR));
        }
        Self { fill, border }
    }

    /// 某轨填充色（不透明）。
    #[inline]
    #[must_use]
    pub fn fill(&self, track: usize) -> u32 {
        match self.fill.get(track) {
            Some(&color) => color,
            None => linear_to_argb(palette_linear(track)),
        }
    }

    /// 某轨边框色（不透明，线性空间 ×0.4）。
    #[inline]
    #[must_use]
    pub fn border(&self, track: usize) -> u32 {
        match self.border.get(track) {
            Some(&color) => color,
            None => linear_scaled_argb(palette_linear(track), BORDER_DARKEN_FACTOR),
        }
    }

    /// 某轨填充色（RGBA8）。
    #[inline]
    #[must_use]
    pub fn fill_rgba(&self, track: usize) -> [u8; 4] {
        argb_to_rgba(self.fill(track))
    }

    /// 某轨边框色（RGBA8）。
    #[inline]
    #[must_use]
    pub fn border_rgba(&self, track: usize) -> [u8; 4] {
        argb_to_rgba(self.border(track))
    }
}

/// 取当前调色板轨色（线性 RGB，忽略 alpha；与 GPU `unpack_key_color` 同口径）。
#[inline]
fn palette_linear(track: usize) -> [f32; 3] {
    let color = palette::current_track_color_f32(track);
    [color[0], color[1], color[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_endpoints() {
        assert_eq!(srgb_encode(0.0), 0);
        assert_eq!(srgb_encode(1.0), 255);
        assert_eq!(srgb_decode(0), 0.0);
        assert_eq!(srgb_decode(255), 1.0);
    }

    #[test]
    fn main_track_colors_match_shader_math() {
        let style = NoteStyle::new(1);
        assert_eq!(
            linear_to_argb(MAIN_TRACK_LINEAR),
            pack_argb(124, 196, 255, 255)
        );
        assert_eq!(
            linear_scaled_argb(MAIN_TRACK_LINEAR, BORDER_DARKEN_FACTOR),
            pack_argb(80, 129, 170, 255)
        );
        assert_eq!(style.fill_rgba(0)[3], 255);
        assert_eq!(style.border_rgba(0)[3], 255);
    }

    #[test]
    fn style_falls_back_beyond_cached_track_count() {
        let style = NoteStyle::new(0);
        let fill = style.fill(7);
        let border = style.border(7);
        assert_eq!(argb_to_rgba(fill)[3], 255);
        assert_eq!(argb_to_rgba(border)[3], 255);
        assert_ne!(fill, border);
    }

    #[test]
    fn argb_roundtrip() {
        let pixel = pack_argb(1, 2, 3, 4);
        assert_eq!(argb_to_rgba(pixel), [1, 2, 3, 4]);
    }
}
