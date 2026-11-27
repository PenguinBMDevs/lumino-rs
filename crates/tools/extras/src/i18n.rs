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

/// 获取主界面翻译（`main` 模块的入口原样转出）
pub use main::get as main_translations;
/// 获取设置面板翻译（`settings` 模块的入口原样转出）
pub use settings::get as settings_translations;

/// 主界面的格式化/枚举名称辅助函数：**原样转出**，不再逐层包一层。
///
/// 此前 7 个 `pub fn x(..) { main::x(..) }` 只是转发；`pub use` 保持
/// `lumino_extras::i18n::x` 路径与签名不变，同时删掉样板。
pub use main::{
    dot_type_name, eraser_behavior_name, format_thousands, note_precision_name,
    selection_box_mode_name, synth_backend_name, track_add_behavior_name,
};

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
