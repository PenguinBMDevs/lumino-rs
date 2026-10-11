//! 系统语言检测与界面语言偏好（UI-018）
//!
//! 需求：中文系系统（简体 / 繁体 / 其他体）显示中文，其他语言（日语 / 英语 / 韩语 …）
//! 显示英语。调研与实测证据见
//! `docs/2026-10-11-UI-018-语言自动跟随系统切换调研.md`。
//!
//! ## 为什么必须按「主语言子标签」匹配
//!
//! 同一台机器上，Windows 的三个来源给出的写法并不一致（本机 Windows 11 实测）：
//!
//! | 来源 | 简体中文系统上的值 |
//! |---|---|
//! | `GetUserPreferredUILanguages(MUI_LANGUAGE_NAME)`（本模块实际走的口径） | `zh-CN` |
//! | Windows 11「语言和区域」设置 / WinRT | `zh-Hans-CN` |
//! | 官方语言包列表（同族其它地区） | `zh-Hans-MO` / `zh-Hant-MO` / `zh-Hant-TW` |
//!
//! 因此**整串相等匹配必然漏判**：本模块一律取第一个 `-` / `_` 之前的主子标签，
//! 再做 `zh` / 非 `zh` 二分。繁体（`zh-Hant-*`）目前也归入 [`Language::ZhCn`]——
//! 仓库只有简体译文，产品口径为「繁体系统显示简体」。
//!
//! ## 平台取值
//!
//! 统一走 [`sys_locale`]：Windows = 用户**显示语言**列表
//! （`GetUserPreferredUILanguages`，不是区域格式）、macOS =
//! `CFLocaleCopyPreferredLanguages`、Linux/BSD = `LANGUAGE > LC_ALL > LC_MESSAGES > LANG`。
//! 该 crate 早已由 `cosmic-text` 传递引入且每次启动都在执行（字体回退按 locale 选字体），
//! 提为直接依赖不新增任何供应链。

use crate::types::Language;

/// 语言 tag 里「无法判定语言」的占位值（容器 / CI / SSH 下 `LANG=C` 极常见）。
const UNKNOWN_TAGS: [&str; 2] = ["c", "posix"];

/// 界面语言偏好：跟随系统，或固定为某语言。
///
/// 序列化为 `{"mode":"system"}` / `{"mode":"fixed","language":"en-US"}`。显式带
/// tag 是为了让「偏好」与「生效语言」（`UiConfig::language`）在 config.json 里
/// 一眼可分，也避免将来新增模式时与既有取值撞车。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode", content = "language")]
pub enum LanguagePreference {
    /// 跟随系统显示语言（默认）
    #[default]
    System,
    /// 固定为指定语言（用户在设置面板显式选择）
    Fixed(Language),
}

impl LanguagePreference {
    /// 解析为生效语言；`System` 每次实时探测系统语言。
    pub fn resolve(self) -> Language {
        match self {
            Self::Fixed(lang) => lang,
            Self::System => detect_system_language(),
        }
    }

    /// 是否为「跟随系统」。
    pub fn is_system(self) -> bool {
        matches!(self, Self::System)
    }
}

/// 取语言 tag 的主语言子标签（ASCII 小写）；无法判定返回 `None`。
///
/// 处理 POSIX 编码与修饰符（`zh_CN.UTF-8@euro` → `zh`）、`_` 分隔、大小写，
/// 以及占位值 `C` / `POSIX`。
fn primary_subtag(tag: &str) -> Option<String> {
    let head = tag.split(['.', '@']).next().unwrap_or("").trim();
    let primary = head.split(['-', '_']).next().unwrap_or("").trim();
    let lower = primary.to_ascii_lowercase();
    if lower.is_empty() || UNKNOWN_TAGS.contains(&lower.as_str()) {
        return None;
    }
    Some(lower)
}

impl Language {
    /// 语言的自称（设置面板下拉列表用）。
    ///
    /// 与 [`std::fmt::Display`] 同一来源，避免两处文案漂移。
    pub fn name(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::EnUs => "English",
        }
    }

    /// 按语言 tag 解析界面语言；无法判定返回 `None`。
    ///
    /// `zh*` → [`Language::ZhCn`]；其余一切已识别 tag → [`Language::EnUs`]
    /// （需求口径：非中文系系统显示英语）。
    pub fn from_tag(tag: &str) -> Option<Self> {
        match primary_subtag(tag)?.as_str() {
            "zh" => Some(Self::ZhCn),
            _ => Some(Self::EnUs),
        }
    }
}

/// 按偏好降序取第一个可判定的 tag（RFC 4647 §3.4 Lookup 在二语言下的退化形式）。
///
/// 取「第一个可判定」而不是「最高优先级的中文」：Windows 的列表语义是
/// 「主显示语言 + en-US」，首项即用户当前显示语言；若首项是 `ja-JP`，就该给英语。
pub fn language_from_tags<I, S>(tags: I) -> Option<Language>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    tags.into_iter()
        .find_map(|tag| Language::from_tag(tag.as_ref()))
}

/// 系统当前的显示语言 tag 列表（按偏好降序）。
pub fn system_language_tags() -> Vec<String> {
    sys_locale::get_locales().collect()
}

/// 探测系统显示语言，并返回实际命中的 tag（无法判定时为 `None`），供日志定位。
pub fn detect_system_language_with_tag() -> (Language, Option<String>) {
    for tag in system_language_tags() {
        if let Some(lang) = Language::from_tag(&tag) {
            return (lang, Some(tag));
        }
    }
    (Language::default(), None)
}

/// 探测系统显示语言；无法判定时回落到 [`Language::default`]（简体中文）。
///
/// 「无法判定」只发生在 tag 列表为空或全为 `C` / `POSIX` 时——此时保持仓库默认
/// 中文，而不是把中文用户切成英文（避免在容器 / 无 locale 环境制造回归）。
pub fn detect_system_language() -> Language {
    detect_system_language_with_tag().0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机实测口径的映射表（Windows 11 简体系统实际出现的全部写法 + 常见他国语言）。
    ///
    /// 左列 = 输入 tag，右列 = 期望语言。
    const CASES: &[(&str, Language)] = &[
        // ── 简体（`zh-CN` 是 MUI API 返回值，`zh-Hans-CN` 是 Win11 设置里的写法）──
        ("zh-CN", Language::ZhCn),
        ("zh-Hans-CN", Language::ZhCn),
        ("zh-Hans", Language::ZhCn),
        ("zh-Hans-SG", Language::ZhCn),
        ("zh-SG", Language::ZhCn),
        ("zh-Hans-MY", Language::ZhCn),
        ("zh-Hans-MO", Language::ZhCn),
        ("ZH-hans", Language::ZhCn),
        ("zh", Language::ZhCn),
        // ── 繁体：仓库只有简体译文，产品口径为「显示简体」──
        ("zh-TW", Language::ZhCn),
        ("zh-Hant-TW", Language::ZhCn),
        ("zh-HK", Language::ZhCn),
        ("zh-Hant-HK", Language::ZhCn),
        ("zh-Hant-MO", Language::ZhCn),
        ("zh-Hant", Language::ZhCn),
        // ── POSIX 形态 ──
        ("zh_CN.UTF-8", Language::ZhCn),
        ("zh_CN.UTF-8@euro", Language::ZhCn),
        ("zh_TW.Big5", Language::ZhCn),
        // ── 非中文系：一律英语 ──
        ("en-US", Language::EnUs),
        ("en-GB", Language::EnUs),
        ("en", Language::EnUs),
        ("ja-JP", Language::EnUs),
        ("ja", Language::EnUs),
        ("ko-KR", Language::EnUs),
        ("ko", Language::EnUs),
        ("de-DE", Language::EnUs),
        ("fr-FR", Language::EnUs),
        ("ru-RU.UTF-8", Language::EnUs),
        ("ar-SA", Language::EnUs),
        ("pt-BR", Language::EnUs),
        ("tr-TR", Language::EnUs),
        ("vi-VN", Language::EnUs),
        ("th-TH", Language::EnUs),
        // 无法识别的 tag 按「非中文」处理 → 英语（需求：其他语言显示英语）
        ("x-private", Language::EnUs),
        ("und", Language::EnUs),
    ];

    #[test]
    fn test_language_from_tag_table() {
        for (tag, expected) in CASES {
            assert_eq!(
                Language::from_tag(tag),
                Some(*expected),
                "tag {tag:?} 解析结果与期望不符"
            );
        }
    }

    #[test]
    fn test_language_from_tag_unjudgeable() {
        // 容器 / CI / SSH 的 `LANG=C`、空串、空白：一律判定不出来（不猜语言）。
        for tag in [
            "", "   ", "C", "c", "POSIX", "posix", ".UTF-8", "@euro", "-",
        ] {
            assert_eq!(Language::from_tag(tag), None, "tag {tag:?} 不应被判定");
        }
    }

    #[test]
    fn test_language_from_tags_prefers_first_judgeable() {
        // Windows 实际列表语义：主显示语言在前。
        assert_eq!(language_from_tags(["zh-CN", "en-US"]), Some(Language::ZhCn));
        assert_eq!(language_from_tags(["ja-JP", "en-US"]), Some(Language::EnUs));
        // 首项判定不出（如 `LANG=C` 前置）时继续看下一项。
        assert_eq!(language_from_tags(["C", "zh-CN"]), Some(Language::ZhCn));
        assert_eq!(language_from_tags(["C", "POSIX"]), None);
        assert_eq!(language_from_tags(Vec::<String>::new()), None);
    }

    #[test]
    fn test_detect_system_language_always_returns_supported() {
        // ⚠️ 刻意**不**断言具体值：CI / 开发机的系统语言各不相同，
        // 断言具体语言必然假失败。这里只保证「一定是受支持的语言」。
        for lang in [
            detect_system_language(),
            detect_system_language_with_tag().0,
        ] {
            assert!(Language::all().contains(&lang));
        }
    }

    #[test]
    fn test_language_preference_resolve() {
        assert_eq!(
            LanguagePreference::Fixed(Language::EnUs).resolve(),
            Language::EnUs
        );
        // System 走实时探测，只断言落在受支持集合内。
        assert!(Language::all().contains(&LanguagePreference::System.resolve()));
        assert!(LanguagePreference::default().is_system());
    }

    #[test]
    fn test_language_preference_serde_shape() {
        let system =
            serde_json::to_string(&LanguagePreference::System).expect("序列化 System 失败");
        assert_eq!(system, r#"{"mode":"system"}"#);

        let fixed = serde_json::to_string(&LanguagePreference::Fixed(Language::EnUs))
            .expect("序列化 Fixed 失败");
        assert_eq!(fixed, r#"{"mode":"fixed","language":"en-US"}"#);

        let back: LanguagePreference = serde_json::from_str(&fixed).expect("反序列化 Fixed 失败");
        assert_eq!(back, LanguagePreference::Fixed(Language::EnUs));

        let back: LanguagePreference = serde_json::from_str(&system).expect("反序列化 System 失败");
        assert_eq!(back, LanguagePreference::System);
    }

    #[test]
    fn test_language_name_matches_display() {
        for lang in Language::all() {
            assert_eq!(lang.to_string(), lang.name());
        }
    }
}
