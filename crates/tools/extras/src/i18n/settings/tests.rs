use super::*;
use lumino_core::types::Language;

#[test]
fn test_settings_translations_zhcn() {
    let t = get(Language::ZhCn);
    assert_eq!(t.general, "常规");
    assert_eq!(t.confirm, "确认");
    assert_eq!(t.cancel, "取消");
    assert_eq!(t.app_name, "Lumino");
}

#[test]
fn test_settings_translations_enus() {
    let t = get(Language::EnUs);
    assert_eq!(t.general, "General");
    assert_eq!(t.confirm, "Confirm");
    assert_eq!(t.cancel, "Cancel");
    assert_eq!(t.app_name, "Lumino");
}

#[test]
fn test_settings_translations_not_empty() {
    for lang in [Language::ZhCn, Language::EnUs] {
        let t = get(lang);
        assert!(!t.general.is_empty());
        assert!(!t.confirm.is_empty());
        assert!(!t.audio_title.is_empty());
        assert!(!t.ui_title.is_empty());
        assert!(!t.about_title.is_empty());
        assert!(!t.compatibility.is_empty());
        assert!(!t.compat_check_button.is_empty());
        assert!(!t.compat_domino_clipboard.is_empty());
        assert!(!t.compat_domino_clipboard_hint.is_empty());
        assert!(!t.compat_domino_clipboard_unsupported_hint.is_empty());
        // UI-018：语言「跟随系统」选项
        assert!(!t.language_follow_system.is_empty());
        // 加载保护：两个开关与说明必须都有文案（缺一个就是空标签/空提示）
        assert!(!t.load_guard_section.is_empty());
        assert!(!t.load_memory_check.is_empty());
        assert!(!t.load_memory_check_hint.is_empty());
        assert!(!t.load_watchdog.is_empty());
        assert!(!t.load_watchdog_hint.is_empty());
        assert!(!t.load_watchdog_unsupported_hint.is_empty());
    }
}

#[test]
fn test_language_follow_system_option_is_localized() {
    assert_eq!(get(Language::ZhCn).language_follow_system, "跟随系统");
    assert_eq!(get(Language::EnUs).language_follow_system, "Follow System");
}
