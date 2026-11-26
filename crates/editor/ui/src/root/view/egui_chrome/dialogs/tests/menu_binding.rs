//! 设置菜单索引与 egui 菜单表的绑定校验。
//!
//! 这些常量不是纯展示数据：
//! - `MENU_INDEX_ABOUT` 是回声洞打字机的唯一装填信号；
//! - `MENU_INDEX_COMPATIBILITY` 是 GPU 检测结果生命周期的判定依据。
//!
//! 索引漂移不会产生编译错误，只会让功能静默落到错误的页面上。

use lumino_extras::i18n::{Language, settings_translations};

use super::settings_labels;

/// `MENU_INDEX_ABOUT` 必须与 egui 设置菜单表**绑定**（原 上一代 菜单表已删除，
/// 绑定校验迁到这里——`settings_labels` 是菜单的真实数据源）。
///
/// 漂移后果：索引 9 指向别的页时，关于页的回声洞打字机装填点会落到
/// 不存在的页面，且不会有任何编译错误。
#[test]
fn settings_menu_binds_about_index() {
    for language in [Language::ZhCn, Language::EnUs] {
        let labels = settings_labels(language);
        let t = settings_translations(language);
        let index = crate::settings::MENU_INDEX_ABOUT;
        assert!(
            index < labels.menu.len(),
            "MENU_INDEX_ABOUT({index}) 越界（菜单共 {} 项）",
            labels.menu.len()
        );
        assert_eq!(
            labels.menu[index], t.about,
            "MENU_INDEX_ABOUT({index}) 指向的不是「关于」页：彩蛋装填点已漂移"
        );
        assert_eq!(
            labels
                .menu
                .iter()
                .filter(|label| **label == t.about)
                .count(),
            1,
            "「关于」项必须唯一"
        );
    }
}

/// `MENU_INDEX_COMPATIBILITY` 必须与 egui 设置菜单表绑定：Runner 依赖它
/// 判断「GPU 检测结果是否仍属于当前页面」。
///
/// 漂移后果：切页后旧结果会继续展示（或兼容性页的结果被误清），
/// 且不会有任何编译错误。
#[test]
fn settings_menu_binds_compatibility_index() {
    for language in [Language::ZhCn, Language::EnUs] {
        let labels = settings_labels(language);
        let t = settings_translations(language);
        let index = crate::settings::MENU_INDEX_COMPATIBILITY;
        assert!(
            index < labels.menu.len(),
            "MENU_INDEX_COMPATIBILITY({index}) 越界（菜单共 {} 项）",
            labels.menu.len()
        );
        assert_eq!(
            labels.menu[index], t.compatibility,
            "MENU_INDEX_COMPATIBILITY({index}) 指向的不是「兼容性」页：结果生命周期判定已漂移"
        );
        assert_eq!(
            labels
                .menu
                .iter()
                .filter(|label| **label == t.compatibility)
                .count(),
            1,
            "「兼容性」项必须唯一"
        );
    }
}
