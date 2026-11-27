//! 关于对话框：模型镜像、版本号来源与帮助菜单接线。

use super::*;
use lumino_ui_egui::chrome::{DialogModel, MenuEntry};

use crate::Message;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::About,
    )
}

#[test]
fn about_dialog_model_carries_brand_and_version() {
    let root = dialog_root();
    let model = root.egui_chrome_model();
    let Some(DialogModel::About(about)) = model.dialog else {
        panic!("关于窗口应由 egui 整窗接管");
    };

    assert_eq!(about.title, "Lumino - 新一代MIDI编辑器");
    assert!(
        about.version.contains(crate::APP_VERSION),
        "版本号必须取编译期 APP_VERSION，实际为 {:?}",
        about.version
    );
    assert!(
        about.titlebar,
        "默认配置使用自制标题栏：顶部 30px 由 egui 的 popup_titlebar 绘制"
    );
}

#[test]
fn help_menu_about_item_emits_about_event() {
    let root = Root::new(&lumino_core::storage::config::UiConfig::default());
    let bar = root
        .egui_chrome_model()
        .titlebar
        .expect("自绘标题栏应有模型")
        .menu
        .expect("应带可交互菜单树");
    let help_menu = bar.menus.last().expect("应有帮助菜单");
    assert_eq!(
        help_menu.label,
        lumino_core::i18n::main_translations(lumino_core::i18n::Language::ZhCn).menu_help
    );

    let MenuEntry::Action { label, action, .. } = &help_menu.entries[0] else {
        panic!("帮助菜单第一项应为可点击动作");
    };
    assert_eq!(
        *label,
        lumino_core::i18n::main_translations(lumino_core::i18n::Language::ZhCn).help_about
    );
    assert!(
        matches!(
            root.menu_action_message(*action),
            Some(Message::Core(crate::event::Event::Menu(
                crate::event::menu::Event::Help(crate::event::menu::help::Event::About)
            )))
        ),
        "帮助 → 关于必须解析为 About 事件"
    );
}
