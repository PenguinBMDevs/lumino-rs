//! 字体覆盖探针 —— 彩蛋文案码点可得性（不入库：`.gitignore` 忽略 `examples/`）
//!
//! 用法：`cargo run -p lumino-note-core --example echo_glyph_probe`
//!
//! 目的：回答「内置文案里的 🐧（U+1F427）在系统字体链里到底有没有字形」。
//! 判定口径 = `ab_glyph::Font::glyph_id(c) != 0`（0 即 .notdef，无字形）。
//!
//! 与 `font_scanner.rs` 同源：枚举走 `scan_system_fonts()`（fontdb），不再依赖 font-kit
//! （font-kit 在 Android 上强编 fontconfig-sys，已整体移除）。

use ab_glyph::{Font as _, FontVec};

use lumino_note_core::font_scanner;

/// 关心的字体家族：默认 UI 字体 + cosmic-text Windows 回退链
/// （`cosmic-text-0.15.0/src/font/fallback/windows.rs:30-39,46-60`）
const WATCHED: &[&str] = &[
    "Microsoft YaHei",    // config 默认值（英文名）
    "Microsoft YaHei UI", // Han 脚本回退目标（windows.rs:58）
    "Segoe UI",           // common_fallback[0]
    "Segoe UI Emoji",     // common_fallback[1]
    "Segoe UI Symbol",    // common_fallback[2]
    "Noto Color Emoji",
];

/// 待测码点：彩蛋 5 条文案用到的全部非 ASCII 字符 + 基线 + 阴性对照
const TARGETS: &[(char, &str)] = &[
    ('A', "ASCII 基线"),
    ('\u{E000}', "私用区（阴性对照，应几乎无命中）"),
    ('\u{1F427}', "🐧 企鹅（待判定）"),
    ('\u{4F60}', "你（文案汉字）"),
    ('\u{56DE}', "回（文案汉字）"),
    ('\u{9F50}', "齐（文案汉字）"),
    ('\u{2014}', "— 全角破折号"),
    ('\u{2026}', "… 省略号"),
    ('\u{FF01}', "！ 全角感叹号"),
    ('\u{FF08}', "（ 全角左括号"),
    ('\u{FF1A}', "： 全角冒号"),
];

/// 按路径加载字体（字体集合取第 0 个字面）
fn load(path: &std::path::Path) -> Option<FontVec> {
    let bytes = std::fs::read(path).ok()?;
    FontVec::try_from_vec_and_index(bytes, 0).ok()
}

fn main() {
    let fonts = font_scanner::scan_system_fonts();
    println!("系统字体家族数: {}", fonts.len());

    let loaded: Vec<(&str, FontVec)> = fonts
        .iter()
        .filter_map(|info| load(&info.path).map(|font| (info.name.as_str(), font)))
        .collect();
    println!("成功加载 face: {}", loaded.len());

    let watched: Vec<(&str, Option<&FontVec>)> = WATCHED
        .iter()
        .map(|name| {
            (
                *name,
                loaded
                    .iter()
                    .find(|(family, _)| family.eq_ignore_ascii_case(name))
                    .map(|(_, font)| font),
            )
        })
        .collect();

    println!("关注字体（✓存在 / —系统中无此家族）:");
    for (name, font) in &watched {
        println!(
            "  {:<20} {}",
            name,
            if font.is_some() {
                "已加载"
            } else {
                "缺失"
            }
        );
    }
    println!();

    println!(
        "{:<8} {:<24} {:>6}  关注字体逐项（顺序同上）",
        "码点", "含义", "命中家族"
    );
    println!("{}", "-".repeat(118));

    let mut emoji_hits: Vec<&str> = Vec::new();

    for (ch, label) in TARGETS {
        let hits: Vec<&str> = loaded
            .iter()
            .filter(|(_, font)| font.glyph_id(*ch).0 != 0)
            .map(|(name, _)| *name)
            .collect();

        let marks: String = watched
            .iter()
            .map(|(_, font)| match font {
                Some(font) if font.glyph_id(*ch).0 != 0 => "✓",
                Some(_) => "✗",
                None => "—",
            })
            .collect::<Vec<_>>()
            .join(" ");

        println!(
            "U+{:04X}   {:<24} {:>6}  {}",
            *ch as u32,
            label,
            hits.len(),
            marks
        );

        if *ch == '\u{1F427}' {
            emoji_hits = hits;
        }
    }

    println!("\n── 判定 ──");
    if emoji_hits.is_empty() {
        println!("🐧 在**全部**系统字体家族中均无字形 ⇒ 必然渲染为 .notdef（豆腐块/空洞）。");
        println!("结论：文案不得依赖 🐧，需退化。");
    } else {
        println!(
            "🐧 命中 {} 个家族 ⇒ 系统内**有**可用字形，需依赖 cosmic-text 逐字形回退。",
            emoji_hits.len()
        );
        let emoji_like: Vec<&str> = emoji_hits
            .iter()
            .filter(|name| {
                let name = name.to_ascii_lowercase();
                name.contains("emoji") || name.contains("symbol") || name.contains("segui")
            })
            .copied()
            .collect();
        println!("其中疑似 emoji/符号字体: {}", emoji_like.join(", "));
        println!(
            "链路：[1] 字形存在 ← 本探针；[2] 回退可达 ← cosmic-text fallback 链；[3] 彩色光栅化 ← cryoglyph"
        );
    }
}
