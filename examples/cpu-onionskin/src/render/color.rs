//! 音符颜色 / 几何风格 —— 与当前 GPU 渲染逐项对齐。
//!
//! 当前 UI 卷帘的两个事实（源码为准，不按注释猜）：
//! - 主轨 / 洋葱皮实例的 `border_width` **低 16 位固定为 1**
//!   （`note_update.rs::MAIN_TRACK_BORDER_WIDTH`、
//!   `onion_skin.rs::ONION_SKIN_BORDER_WIDTH`）；
//! - 着色器 `horiz_margin = border_px / (width*0.5)`，所以**每侧实际边框 = 2px**，
//!   注释里的“2 像素描边”指的就是这个效果。
//!
//! 颜色：shader 输出线性值到 sRGB render target（`Context` 选择 `is_srgb` 格式），
//! 因此 CPU 端必须做 linear → sRGB 编码才能与屏幕像素一致；主轨蓝
//! `(0.2, 0.55, 1.0)`、边框 `color * 0.4` 均在线性空间计算后再编码。

use lumino_extras::palette;

/// UI 侧固定边框字段值（低 16 位）；实际每侧边框 = 2 × 本值。
pub const NOTE_BORDER_WIDTH: u32 = 1;
/// 着色器里每侧边框的实际像素数。
pub const NOTE_BORDER_PX: f32 = NOTE_BORDER_WIDTH as f32 * 2.0;
/// 边框加深因子（线性空间）。
pub const BORDER_DARKEN_FACTOR: f32 = 0.4;
/// 预览音符 alpha。
#[allow(dead_code)]
pub const PREVIEW_ALPHA: f32 = 0.7;
/// 主轨固定蓝（线性，与 `onion_note.wgsl::MAIN_TRACK_COLOR` 一致）。
pub const MAIN_TRACK_LINEAR: [f32; 3] = [0.2, 0.55, 1.0];

/// 线性值 → sRGB 字节。
#[must_use]
pub fn srgb_encode(linear: f32) -> u8 {
    let v = linear.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// sRGB 字节 → 线性值。
#[must_use]
pub fn srgb_decode(value: u8) -> f32 {
    let v = value as f32 / 255.0;
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// 三个 sRGB 字节打包为 0x00RRGGBB（softbuffer 像素格式）。
#[must_use]
pub const fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// 线性 RGB → 打包 sRGB 像素。
#[must_use]
pub fn linear_to_pixel(rgb: [f32; 3]) -> u32 {
    pack_rgb(
        srgb_encode(rgb[0]),
        srgb_encode(rgb[1]),
        srgb_encode(rgb[2]),
    )
}

/// 线性 RGB × 系数后编码。
#[must_use]
pub fn linear_scaled_pixel(rgb: [f32; 3], factor: f32) -> u32 {
    linear_to_pixel([rgb[0] * factor, rgb[1] * factor, rgb[2] * factor])
}

/// 每轨颜色缓存（填充 / 边框）。
#[derive(Debug, Clone)]
pub struct NoteStyle {
    fill: Vec<u32>,
    border: Vec<u32>,
    /// 主轨固定蓝填充。
    pub main_fill: u32,
    /// 主轨固定蓝边框。
    pub main_border: u32,
}

impl NoteStyle {
    /// 按当前调色板构建所有音轨颜色。
    ///
    /// 与洋葱皮流一致：第 `track` 轨取 `current_track_color_f32(track)`；
    /// 主轨在渲染时强制使用固定蓝，不依赖调色板。
    #[must_use]
    pub fn new(track_count: usize) -> Self {
        let mut fill = Vec::with_capacity(track_count);
        let mut border = Vec::with_capacity(track_count);
        for track in 0..track_count {
            let color = palette::current_track_color_f32(track);
            fill.push(linear_to_pixel([color[0], color[1], color[2]]));
            border.push(linear_scaled_pixel(
                [color[0], color[1], color[2]],
                BORDER_DARKEN_FACTOR,
            ));
        }
        let main_fill = linear_to_pixel(MAIN_TRACK_LINEAR);
        let main_border = linear_scaled_pixel(MAIN_TRACK_LINEAR, BORDER_DARKEN_FACTOR);
        Self {
            fill,
            border,
            main_fill,
            main_border,
        }
    }

    /// 某轨填充色。
    #[inline]
    #[must_use]
    pub fn fill(&self, track: usize, is_main: bool) -> u32 {
        if is_main {
            self.main_fill
        } else {
            self.fill.get(track).copied().unwrap_or(self.main_fill)
        }
    }

    /// 某轨边框色。
    #[inline]
    #[must_use]
    pub fn border(&self, track: usize, is_main: bool) -> u32 {
        if is_main {
            self.main_border
        } else {
            self.border.get(track).copied().unwrap_or(self.main_border)
        }
    }
}

/// 预览音符叠色：线性空间 alpha blend 后再编码（GPU sRGB target 的同口径）。
#[must_use]
#[allow(dead_code)]
pub fn blend_preview(dst: u32, src: u32) -> u32 {
    blend_pixel(dst, src, PREVIEW_ALPHA)
}

/// 快速 70% 预览叠色（sRGB 整数近似，用于拖动 ghost/框选覆盖层）。
#[inline]
#[must_use]
pub fn blend_preview_fast(dst: u32, src: u32) -> u32 {
    let mix = |shift: u32| {
        let d = (dst >> shift) & 0xFF;
        let s = (src >> shift) & 0xFF;
        (d * 3 + s * 7) / 10
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

/// 快速半透明叠色（框选矩形用）。
#[inline]
#[must_use]
pub fn blend_overlay_fast(dst: u32, src: u32, src_weight: u32) -> u32 {
    let mix = |shift: u32| {
        let d = (dst >> shift) & 0xFF;
        let s = (src >> shift) & 0xFF;
        (d * (10 - src_weight) + s * src_weight) / 10
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

/// 线性空间 alpha blend（两端均为打包 sRGB 像素）。
#[must_use]
pub fn blend_pixel(dst: u32, src: u32, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let dr = srgb_decode(((dst >> 16) & 0xFF) as u8);
    let dg = srgb_decode(((dst >> 8) & 0xFF) as u8);
    let db = srgb_decode((dst & 0xFF) as u8);
    let sr = srgb_decode(((src >> 16) & 0xFF) as u8);
    let sg = srgb_decode(((src >> 8) & 0xFF) as u8);
    let sb = srgb_decode((src & 0xFF) as u8);
    pack_rgb(
        srgb_encode(dr + (sr - dr) * a),
        srgb_encode(dg + (sg - dg) * a),
        srgb_encode(db + (sb - db) * a),
    )
}

/// 选中态高亮：向白色线性混合 35%（只用于 UI 反馈，不参与"未选中风格"验收）。
#[must_use]
#[allow(dead_code)]
pub fn selected_tint(color: u32) -> u32 {
    blend_pixel(color, pack_rgb(255, 255, 255), 0.35)
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
        assert_eq!(NoteStyle::new(1).main_fill, pack_rgb(124, 196, 255));
        assert_eq!(NoteStyle::new(1).main_border, pack_rgb(80, 129, 170));
    }

    #[test]
    fn preview_blend_is_between_endpoints() {
        let black = pack_rgb(0, 0, 0);
        let white = pack_rgb(255, 255, 255);
        let mixed = blend_preview(black, white);
        assert_ne!(mixed, black);
        assert_ne!(mixed, white);
        let mid = blend_preview(black, white);
        assert!(mid > black && mid < white);
    }

    /// 源码头是唯一事实：shader / UI 常量改了就红。
    #[test]
    fn style_constants_match_production_sources() {
        let onion = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/editor/gfx/src/shaders/onion_note.wgsl"
        ));
        assert!(onion.contains("vec3<f32>(0.2, 0.55, 1.0)"));
        assert!(onion.contains("BORDER_DARKEN_FACTOR: f32 = 0.4"));

        let note = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/editor/gfx/src/shaders/note.wgsl"
        ));
        assert!(note.contains("PREVIEW_ALPHA: f32 = 0.7"));
        assert!(note.contains("BORDER_DARKEN_FACTOR: f32 = 0.4"));

        let note_update = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/editor/ui/src/host/render/separate_thread/runner/note_update.rs"
        ));
        assert!(note_update.contains("MAIN_TRACK_BORDER_WIDTH: u32 = 1"));

        let onion_skin = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/editor/ui/src/host/render/onion_skin.rs"
        ));
        assert!(onion_skin.contains("ONION_SKIN_BORDER_WIDTH: u32 = 1"));
    }
}
