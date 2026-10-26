//! 运行时文案目录（i18n 长尾覆盖层）
//!
//! ## 为什么需要它
//!
//! 项目既有 i18n 走 `lumino-extras::i18n` 的**编译期结构体表**
//! （`MainTranslations` / `SettingsTranslations`），适合界面骨架（菜单、工具栏、
//! 设置页）。但界面骨架之外仍有大量硬编码中文：对话框正文、状态提示、错误回显、
//! 枚举 `Display`……逐个加结构体字段不可维护；更关键的是 `lumino-core::i18n` 之外
//! 的低层 crate（network / audio / gfx / dialog）**根本拿不到 `Language`**，
//! 只能把中文写死。
//!
//! 本模块提供 gettext 风格的**原文即 key** 目录：
//!
//! - 简体中文：直接返回原文（不查表、零额外成本）；
//! - English：查 [`catalog`] 静态表，缺失则回退中文（由覆盖率测试兜底，见
//!   `catalog::tests`）。
//!
//! 语言来源是**进程级当前语言**（[`set_current_language`]），在「启动加载配置」与
//! 「设置面板切换语言」两处同步，因此调用点无需层层透传 `Language`——这正是
//! 2000+ 条长尾文案能一次性落地的前提。
//!
//! ## 用法
//!
//! ```ignore
//! use lumino_core::{tr, trf};
//!
//! let label = tr!("云存储提醒");                       // 无占位符
//! let msg = trf!("连接失败：{e}", e = err);            // 命名占位符
//! let tip = trf!("正在上传 {name}（{pct}%）", name = n, pct = 42);
//! ```
//!
//! 注意：`trf!` 的模板使用 `{名字}` 占位（**不是** `format!` 的 `{}`/`{:?}`）。
//! 若某个 key 只想原样显示花括号（例如「支持 {nc} {bpm} 占位符」），用 `tr!` 即可，
//! `tr!` 不做任何替换。

mod catalog;

pub use crate::types::Language;

use std::sync::atomic::{AtomicU8, Ordering};

/// 当前界面语言的进程级缓存（0 = 简体中文，1 = English）。
///
/// 用原子整数而不是 `Mutex<Language>`：读路径在渲染热路径上（每帧每个标签一次），
/// 必须是 `Relaxed` 级别的无锁读。
static CURRENT_LANGUAGE: AtomicU8 = AtomicU8::new(0);

/// `Language` → 原子存储码。
#[inline]
const fn language_code(lang: Language) -> u8 {
    match lang {
        Language::ZhCn => 0,
        Language::EnUs => 1,
    }
}

/// 原子存储码 → `Language`（未知码回退简体中文）。
#[inline]
const fn language_from_code(code: u8) -> Language {
    match code {
        1 => Language::EnUs,
        _ => Language::ZhCn,
    }
}

/// 设置进程级当前语言。
///
/// 调用点只有两处：启动时按配置初始化、设置面板切换语言时同步。
pub fn set_current_language(lang: Language) {
    CURRENT_LANGUAGE.store(language_code(lang), Ordering::Relaxed);
}

/// 读取进程级当前语言。
#[inline]
pub fn current_language() -> Language {
    language_from_code(CURRENT_LANGUAGE.load(Ordering::Relaxed))
}

/// 按指定语言翻译（原文即 key）。
///
/// 中文直接返回原文；英文查目录，缺失回退中文。
#[inline]
pub fn tr_in(zh: &'static str, lang: Language) -> &'static str {
    match lang {
        Language::ZhCn => zh,
        Language::EnUs => catalog::lookup(zh).unwrap_or(zh),
    }
}

/// 按进程级当前语言翻译（原文即 key）。
#[inline]
pub fn tr(zh: &'static str) -> &'static str {
    tr_in(zh, current_language())
}

/// 查英文目录；未登记返回 `None`。
///
/// 供“key 在数据表里、调用点不是 `tr!` 字面量”的场景做覆盖率测试（帮助话题）。
pub fn lookup_en(zh: &str) -> Option<&'static str> {
    catalog::lookup(zh)
}

/// 动态取词：key 是**运行时字符串**（例如界面状态里存的中文数据值）。
///
/// 与 [`tr`] 的差别只在返回类型：`tr` 拿 `&'static str` 可以直接内联，
/// 这里拿 `&str` 只能回传 `String`。
///
/// 典型用途：状态里存的是**规范数据值**（如视频导出的渲染模式 `"计数器"`），
/// 这些值同时被 `match` / `==` 当 key 用、不能随语言变；但下拉列表要显示英文，
/// 于是显示侧用 `tr_dyn(&value)` 取词。
pub fn tr_dyn(zh: &str) -> String {
    match current_language() {
        Language::ZhCn => zh.to_string(),
        Language::EnUs => catalog::lookup(zh)
            .map(str::to_string)
            .unwrap_or_else(|| zh.to_string()),
    }
}

/// 把 `{名字}` 占位符替换为实参。
///
/// - `{name}` / `{0}` 形式均可；数字按 `args` 下标取；
/// - 找不到对应实参的 `{...}` **原样保留**（便于文案里写「支持 {nc} 占位符」）；
/// - `{{` 与 `}}` 转义为单个花括号。
pub fn render(template: &str, args: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let bytes = template.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = template[i..].chars().next().unwrap_or('\0');
        if ch != '{' {
            if ch == '}' {
                // `}}` → `}`
                if template[i + 1..].starts_with('}') {
                    out.push('}');
                    i += 2;
                    continue;
                }
            }
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        // `{{` → `{`
        if template[i + 1..].starts_with('{') {
            out.push('{');
            i += 2;
            continue;
        }
        let Some(close_rel) = template[i + 1..].find('}') else {
            out.push('{');
            i += 1;
            continue;
        };
        let close = i + 1 + close_rel;
        let name = &template[i + 1..close];
        let replacement = args.iter().find(|(k, _)| *k == name).or_else(|| {
            if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
                name.parse::<usize>().ok().and_then(|idx| args.get(idx))
            } else {
                None
            }
        });
        match replacement {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&template[i..=close]),
        }
        i = close + 1;
    }
    out
}

/// 文案目录的只读视图（供审计与测试使用）。
pub mod audit {
    pub use super::catalog::lookup;
}

/// 按进程级当前语言翻译（无占位符）。
#[macro_export]
macro_rules! tr {
    ($zh:literal) => {
        $crate::i18n::tr_in($zh, $crate::i18n::current_language())
    };
}

/// 按进程级当前语言翻译并填充 `{名字}` 占位符。
#[macro_export]
macro_rules! trf {
    ($zh:literal) => {
        $crate::i18n::tr_in($zh, $crate::i18n::current_language()).to_string()
    };
    ($zh:literal, $($key:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::render(
            $crate::i18n::tr_in($zh, $crate::i18n::current_language()),
            &[$((stringify!($key), ($value).to_string())),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_language_roundtrip() {
        // ⚠️ 刻意**不改写**进程级语言：同一测试二进制里还有断言中文 `Display` 输出的用例
        // （如 `types.rs` 的 `test_note_precision_display`），而测试是并行跑的，
        // 一旦它们并发读到 EnUs 就会假失败。这里只验证编解码与默认值本身。
        for lang in Language::all() {
            assert_eq!(language_from_code(language_code(lang)), lang);
        }
        assert_eq!(language_from_code(0), Language::ZhCn, "未知码回退简体中文");
        assert_eq!(current_language(), Language::ZhCn, "默认语言为简体中文");
    }

    #[test]
    fn test_tr_zh_returns_source_text() {
        // 中文侧永不查表：即使目录里没有该 key，也必须原样返回。
        let key = "一条绝对不会进目录的测试文案";
        assert_eq!(tr_in(key, Language::ZhCn), key);
    }

    #[test]
    fn test_render_named_and_numeric_placeholders() {
        let args = [("e", "boom".to_string()), ("0", "zero".to_string())];
        assert_eq!(render("连接失败：{e}", &args), "连接失败：boom");
        assert_eq!(render("第 {0} 项", &args), "第 zero 项");
    }

    #[test]
    fn test_render_keeps_unknown_placeholders() {
        // 文案里的「占位符说明」必须原样保留，不能被替换成空串。
        assert_eq!(
            render("支持 {nc} {bpm} 占位符", &[]),
            "支持 {nc} {bpm} 占位符"
        );
    }

    #[test]
    fn test_render_escaped_braces() {
        assert_eq!(render("a{{b}}c", &[]), "a{b}c");
    }

    #[test]
    fn test_render_multibyte_boundaries() {
        // 确认按字符边界推进，避免中文串被切坏。
        assert_eq!(render("中文{k}尾巴", &[("k", "K".into())]), "中文K尾巴");
    }
}
