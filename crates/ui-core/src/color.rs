//! 共享颜色工具函数

use iced_core::Color;

/// 按背景亮度返回黑色或白色，保证文本对比度
pub fn contrast_text_color(bg: Color) -> Color {
    let luminance = 0.299 * bg.r + 0.587 * bg.g + 0.114 * bg.b;
    if luminance > 0.5 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

/// 两个颜色按 t 比例线性混合
pub fn blend_color(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}
