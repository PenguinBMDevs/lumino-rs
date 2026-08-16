//! 多语言支持模块
//!
//! 提供类型安全的多语言翻译，支持简体中文和 English。
//! 所有翻译字符串集中在此模块管理，避免散落在 UI 代码中。

pub mod main;
pub mod settings;

pub use main::MainTranslations;
pub use settings::SettingsTranslations;

pub use lumino_core::types::Language;
use lumino_core::{DotType, NotePrecision};

/// 获取主界面翻译
pub fn main_translations(lang: Language) -> &'static MainTranslations {
    main::get(lang)
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
