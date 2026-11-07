//! 输出 TokyoNightStorm 的 iced Extended 调色板（供 egui 主题精确对齐）。

use iced_core::Theme;
use iced_core::theme::palette::Extended;

fn hex(color: iced_core::Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8
    )
}

fn main() {
    let ext: &Extended = Theme::TokyoNightStorm.extended_palette();

    println!("is_dark = {}", ext.is_dark);
    println!("bg.base         = {}", hex(ext.background.base.color));
    println!("bg.weakest      = {}", hex(ext.background.weakest.color));
    println!("bg.weaker       = {}", hex(ext.background.weaker.color));
    println!("bg.weak         = {}", hex(ext.background.weak.color));
    println!("bg.neutral      = {}", hex(ext.background.neutral.color));
    println!("bg.strong       = {}", hex(ext.background.strong.color));
    println!("bg.stronger     = {}", hex(ext.background.stronger.color));
    println!("bg.strongest    = {}", hex(ext.background.strongest.color));
    println!("text            = {}", hex(ext.background.base.text));
    println!("primary.base    = {}", hex(ext.primary.base.color));
    println!("primary.weak    = {}", hex(ext.primary.weak.color));
    println!("primary.strong  = {}", hex(ext.primary.strong.color));
    println!("secondary.base  = {}", hex(ext.secondary.base.color));
    println!("secondary.weak  = {}", hex(ext.secondary.weak.color));
    println!("secondary.strong= {}", hex(ext.secondary.strong.color));
    println!("success.base    = {}", hex(ext.success.base.color));
    println!("success.strong  = {}", hex(ext.success.strong.color));
    println!("warning.base    = {}", hex(ext.warning.base.color));
    println!("danger.base     = {}", hex(ext.danger.base.color));
    println!("danger.strong   = {}", hex(ext.danger.strong.color));
}
