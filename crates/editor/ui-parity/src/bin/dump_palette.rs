//! 输出指定 iced 主题的 Extended 调色板（供 egui 主题精确对齐）。
//!
//! 用法：`cargo run -p lumino-ui-parity --bin dump_palette -- [主题名]`
//! 主题名缺省为 `Tokyo Night Storm`（历史基线色板）。

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
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Tokyo Night Storm".to_owned());
    let theme = Theme::ALL
        .iter()
        .find(|theme| theme.to_string() == name)
        .cloned()
        .unwrap_or_else(|| {
            let all: Vec<String> = Theme::ALL.iter().map(|theme| theme.to_string()).collect();
            eprintln!("未知主题 {name:?}；可选：{}", all.join(" | "));
            std::process::exit(2);
        });

    let ext: &Extended = theme.extended_palette();

    println!("theme = {}", theme);
    println!("is_dark = {}", ext.is_dark);
    println!("bg.base         = {}", hex(ext.background.base.color));
    println!("bg.base.text    = {}", hex(ext.background.base.text));
    println!("bg.weakest      = {}", hex(ext.background.weakest.color));
    println!("bg.weaker       = {}", hex(ext.background.weaker.color));
    println!("bg.weak         = {}", hex(ext.background.weak.color));
    println!("bg.weak.text    = {}", hex(ext.background.weak.text));
    println!("bg.neutral      = {}", hex(ext.background.neutral.color));
    println!("bg.strong       = {}", hex(ext.background.strong.color));
    println!("bg.stronger     = {}", hex(ext.background.stronger.color));
    println!("bg.strongest    = {}", hex(ext.background.strongest.color));
    println!("primary.base    = {}", hex(ext.primary.base.color));
    println!("primary.weak    = {}", hex(ext.primary.weak.color));
    println!("primary.strong  = {}", hex(ext.primary.strong.color));
    println!("secondary.base  = {}", hex(ext.secondary.base.color));
    println!("secondary.strong= {}", hex(ext.secondary.strong.color));
    println!("success.base    = {}", hex(ext.success.base.color));
    println!("warning.base    = {}", hex(ext.warning.base.color));
    println!("danger.base     = {}", hex(ext.danger.base.color));
    println!("danger.strong   = {}", hex(ext.danger.strong.color));
}
