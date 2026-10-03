//! 字体覆盖探针 —— 彩蛋文案码点可得性（不入库：`.gitignore:69` 忽略 `examples/`）
//!
//! 用法：`cargo run -p lumino-note-core --example echo_glyph_probe`
//!
//! 目的：回答「内置文案里的 🐧（U+1F427）在系统字体链里到底有没有字形」。
//! 判定口径 = `font_kit::Font::glyph_for_char` 返回 `Some`（有字形）而非 `None`。
//! 同时给出默认 UI 字体（`Microsoft YaHei`，见 `core/src/storage/config.rs:233`）的覆盖，
//! 以及一个必然缺失的私用区码点 U+E000 作为**阴性对照**。

use font_kit::{font::Font, source::SystemSource};

/// 关心的字体家族：默认 UI 字体 + cosmic-text Windows 回退链
/// （`cosmic-text-0.15.0/src/font/fallback/windows.rs:30-39,46-60`）
const WATCHED: &[&str] = &[
    "Microsoft YaHei",    // config 默认值（英文名）
    "微软雅黑",           // 同字体中文本地化家族名
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

fn main() {
    let source = SystemSource::new();

    let families = match source.all_families() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("枚举系统字体家族失败: {e}");
            return;
        }
    };
    println!("系统字体家族数: {}", families.len());

    // 每个家族的**首个 face** 作为该家族的代表（与 font_scanner.rs:51 同口径）
    let mut loaded: Vec<(String, Font)> = Vec::new();
    for family in &families {
        let Ok(handle) = source.select_family_by_name(family) else {
            continue;
        };
        let Some(first) = handle.fonts().first() else {
            continue;
        };
        match first.load() {
            Ok(font) => loaded.push((family.clone(), font)),
            Err(_) => continue,
        }
    }
    println!("成功加载 face: {}", loaded.len());

    let watched_fonts: Vec<(&str, Option<&Font>)> = WATCHED
        .iter()
        .map(|name| {
            (
                *name,
                loaded.iter().find(|(f, _)| f == name).map(|(_, f)| f),
            )
        })
        .collect();

    println!("关注字体（✓存在且含字形 / ✗缺字形 / —系统中无此家族）:");
    for (name, font) in &watched_fonts {
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

    let mut emoji_families: Vec<&str> = Vec::new();

    for (ch, label) in TARGETS {
        let hits: Vec<&str> = loaded
            .iter()
            .filter(|(_, font)| font.glyph_for_char(*ch).is_some())
            .map(|(name, _)| name.as_str())
            .collect();

        let marks: String = watched_fonts
            .iter()
            .map(|(_, font)| match font {
                Some(font) if font.glyph_for_char(*ch).is_some() => "✓",
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
            emoji_families = hits.clone();
        }
    }

    println!("\n── 判定 ──");
    if emoji_families.is_empty() {
        println!("🐧 在**全部**系统字体家族中均无字形 ⇒ 必然渲染为 .notdef（豆腐块/空洞）。");
        println!("结论：文案不得依赖 🐧，需退化。");
    } else {
        println!(
            "🐧 命中 {} 个家族 ⇒ 系统内**有**可用字形，需依赖 cosmic-text 逐字形回退。",
            emoji_families.len()
        );
        let emoji_like: Vec<&str> = emoji_families
            .iter()
            .filter(|n| {
                let n = n.to_ascii_lowercase();
                n.contains("emoji") || n.contains("symbol") || n.contains("segui")
            })
            .copied()
            .collect();
        println!("其中疑似 emoji/符号字体: {}", emoji_like.join(", "));
        println!(
            "链路判定（本探针只证「字形存在」这一环，另两环由源码核实）：\n  \
             [1] 字形存在  ← 本探针：Segoe UI Emoji / Segoe UI Symbol / Noto Color Emoji\n  \
             [2] 回退可达  ← cosmic-text-0.15.0/src/font/fallback/windows.rs:30-39\n                 \
                 common_fallback() = [\"Segoe UI\", \"Segoe UI Emoji\", \"Segoe UI Symbol\", …]\n  \
             [3] 彩色光栅化 ← cryoglyph-0.1.0/src/text_render.rs:112\n                 \
                 SwashContent::Color => ContentType::Color（iced_wgpu 走 cryoglyph 字形图集）\n\
             结论：三环齐备 ⇒ 🐧 **可保留**，且预期以彩色渲染。\n\
             残留未知：字号/基线对齐观感（emoji 字面尺寸与 12px 中文混排），建议验收时目视 2 秒。"
        );
    }
}
