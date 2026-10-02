//! 设置页面 - 关于
//!
//! UI-007：页面最上方展示 Lumino logo（`Icon::LogoInApp`），并承载其彩蛋交互——
//! 单击原地晃动一次；连点累计满阈值（见 `AboutEggState`）触发坠落消失序列。
//! 序列中的飞行、旋转、渐隐由对话框根部的悬浮层渲染（`view::about_egg`），
//! 本页只负责「原位」渲染与点击上报。

use iced_widget::{Column, mouse_area, row, space, text};
use lumino_extras::i18n::settings_translations;
use lumino_ui_core::{
    Element, Message, Theme,
    resources::icon::{self, Icon},
    state::{
        AboutEggState,
        about_egg::{ABOUT_LOGO_SHAKE_AMPLITUDE, ABOUT_LOGO_SIZE},
    },
};

use crate::SettingsPanel;

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};

/// 渲染关于页面
///
/// `egg` 为彩蛋状态机：`is_visible_in_page()` 为假时（本进程内已消失）
/// 不再渲染 logo，页面其余内容保持原位布局。
pub fn view<'a>(settings: &SettingsPanel, egg: &'a AboutEggState) -> Element<'a> {
    let t = settings_translations(settings.display.language);

    let mut content: Column<'a, Message, Theme, lumino_ui_core::Renderer> = Column::new()
        .spacing(SPACING_CONTENT)
        .padding(PADDING_CONTENT);

    // logo 位于本页**所有内容上方**
    if egg.is_visible_in_page() {
        content = content
            .push(logo(egg))
            .push(space().height(SPACING_CONTENT));
    }

    content
        .push(
            text(t.about_title)
                .size(TEXT_SIZE_TITLE)
                .style(create_content_text_style()),
        )
        .push(space().height(20))
        .push(
            text(t.app_name)
                .size(16.0)
                .style(create_content_text_style()),
        )
        .push(
            text(t.version)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        )
        .push(space().height(10))
        .push(
            text(t.app_description)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        )
        .into()
}

/// 渲染关于页 logo（含单击晃动反馈）
///
/// 晃动位移由「左右两个定宽占位器」表达：两侧宽度之和恒为 `2 × 振幅`，
/// 因此位移**不改变行宽**，不会引起页面重排抖动。
///
/// 主题传 `None`：Logo 类图标在明暗主题下均保持原色、不参与反色
/// （见 `resources::icon::should_invert_icon`），无需主题参与。
fn logo<'a>(egg: &'a AboutEggState) -> Element<'a> {
    let amp = ABOUT_LOGO_SHAKE_AMPLITUDE;
    let dx = egg.shake_offset().clamp(-amp, amp);
    let size = ABOUT_LOGO_SIZE as u32;

    let glyph = icon::view_with_size_and_theme(Icon::LogoInApp, size, size, None);
    let shifted = row![
        space().width((amp + dx).max(0.0)),
        glyph,
        space().width((amp - dx).max(0.0)),
    ];

    mouse_area(shifted)
        .on_press(Message::Settings(crate::Event::AboutLogoClicked))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;

    /// 关于页在 logo 可见时应比不可见时多出「logo + 间隔」两个子元素
    fn panel() -> SettingsPanel {
        SettingsPanel::new(&UiConfig::default())
    }

    #[test]
    fn test_about_page_renders() {
        let settings = panel();
        let egg = AboutEggState::new();
        // 仅验证构建不 panic 且类型正确（页面结构断言见 ui-settings 集成测试）
        let _element = view(&settings, &egg);
    }

    #[test]
    fn test_logo_hidden_after_vanish_flag() {
        lumino_ui_core::state::about_egg::mark_logo_vanished();
        let settings = panel();
        let egg = AboutEggState::new();
        assert!(
            !egg.is_visible_in_page(),
            "进程内已消失时，重开设置面板不应再渲染 logo"
        );
        let _element = view(&settings, &egg);
        lumino_ui_core::state::about_egg::reset_logo_vanished();
    }
}
