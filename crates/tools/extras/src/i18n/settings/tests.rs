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
    }
}
