//! 多语言支持模块
//!
//! 两层结构：
//!
//! 1. **编译期结构体表**（本模块的 `MainTranslations` / `SettingsTranslations`）：
//!    界面骨架（菜单、工具栏、设置页）逐字段强类型管理；
//! 2. **运行时文案目录**（`lumino_core::i18n`，原文即 key）：对话框正文、状态提示、
//!    错误回显等长尾文案，低层 crate 也能直接用 [`tr!`](lumino_core::tr) /
//!    [`trf!`](lumino_core::trf) 取词，无需层层透传 `Language`。
//!
//! 本模块把第二层原样转出，方便 UI 侧只 `use lumino_extras::i18n::...` 一处。

pub mod main;
pub mod settings;

pub use main::MainTranslations;
pub use settings::SettingsTranslations;

pub use lumino_core::i18n::{
    Language, current_language, render as render_template, set_current_language, tr, tr_dyn, tr_in,
};
pub use lumino_core::{tr, trf};

use lumino_core::{DotType, NotePrecision};

/// 获取主界面翻译
pub fn main_translations(lang: Language) -> &'static MainTranslations {
    main::get(lang)
}

/// 千分位格式化（`1234567` → `"1,234,567"`；UI-015 音符总量显示）。
pub fn format_thousands(n: u64) -> String {
    main::format_thousands(n)
}

/// 获取音符精度名称（按语言）
pub fn note_precision_name(precision: NotePrecision, lang: Language) -> &'static str {
    main::note_precision_name(precision, lang)
}

/// 获取符点类型名称（按语言）
pub fn dot_type_name(dot_type: DotType, lang: Language) -> &'static str {
    main::dot_type_name(dot_type, lang)
}

/// 获取框选框模式名称（按语言）
pub fn selection_box_mode_name(
    mode: lumino_core::storage::config::SelectionBoxMode,
    lang: Language,
) -> &'static str {
    main::selection_box_mode_name(mode, lang)
}

/// 获取橡皮擦行为名称（按语言）
pub fn eraser_behavior_name(
    behavior: lumino_core::storage::config::EraserBehavior,
    lang: Language,
) -> &'static str {
    main::eraser_behavior_name(behavior, lang)
}

/// 获取音轨添加行为名称（按语言）
pub fn track_add_behavior_name(
    behavior: lumino_core::storage::config::TrackAddBehavior,
    lang: Language,
) -> &'static str {
    main::track_add_behavior_name(behavior, lang)
}

/// 获取合成器后端名称（按语言）
pub fn synth_backend_name(
    backend: lumino_core::storage::config::SynthBackend,
    lang: Language,
) -> &'static str {
    main::synth_backend_name(backend, lang)
}

/// 获取设置面板翻译
pub fn settings_translations(lang: Language) -> &'static SettingsTranslations {
    settings::get(lang)
}

#[cfg(test)]
mod tests {
    use lumino_core::types::Language;

    #[test]
    fn test_language_default() {
        assert_eq!(Language::default(), Language::ZhCn);
    }

    #[test]
    fn test_language_all() {
        let all = Language::all();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&Language::ZhCn));
        assert!(all.contains(&Language::EnUs));
    }

    #[test]
    fn test_language_display() {
        assert_eq!(Language::ZhCn.to_string(), "简体中文");
        assert_eq!(Language::EnUs.to_string(), "English");
    }

    #[test]
    fn test_language_serde() {
        let json =
            serde_json::to_string(&Language::ZhCn).expect("Language::ZhCn 序列化为 JSON 不应失败");
        assert_eq!(json, "\"zh-CN\"");
        let deserialized: Language =
            serde_json::from_str(&json).expect("Language JSON 反序列化不应失败");
        assert_eq!(deserialized, Language::ZhCn);

        let json =
            serde_json::to_string(&Language::EnUs).expect("Language::EnUs 序列化为 JSON 不应失败");
        assert_eq!(json, "\"en-US\"");
        let deserialized: Language =
            serde_json::from_str(&json).expect("Language JSON 反序列化不应失败");
        assert_eq!(deserialized, Language::EnUs);
    }
}
