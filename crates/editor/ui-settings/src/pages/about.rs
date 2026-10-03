//! 设置页面 - 关于
//!
//! UI-007：页面最上方展示 Lumino logo（`Icon::LogoInApp`），并承载其彩蛋交互——
//! 单击原地晃动一次；连点累计满阈值（见 `AboutEggState`）触发坠落消失序列。
//! 序列中的飞行、旋转、渐隐由对话框根部的悬浮层渲染（`view::about_egg`），
//! 本页只负责「原位」渲染与点击上报。
//!
//! UI-006：页面**最后一个文本内容下方**新增「回声洞」彩蛋——进入时以打字机效果逐字
//! 显示，单击后当前内容闪烁 0.5s 退出、下一条再以打字机效果进入（状态机
//! `EchoCaveState`，相位推进在 `handle_animation_tick`）。文案内置在程序内，不做本地化。

use iced_core::{Alignment, Length, mouse};
use iced_widget::{Column, container, mouse_area, row, space, text};
use lumino_extras::i18n::settings_translations;
use lumino_ui_core::{
    Element, Message, Theme,
    resources::icon::{self, Icon},
    state::{
        AboutEggState, EchoCaveState,
        about_egg::{ABOUT_LOGO_SHAKE_AMPLITUDE, ABOUT_LOGO_SIZE},
    },
};

use crate::SettingsPanel;

use super::super::components::constants::*;
use super::super::components::styles::{
    create_content_text_style, create_echo_text_style, create_placeholder_text_style,
};

/// 渲染关于页面
///
/// `egg` 为彩蛋状态机：`is_visible_in_page()` 为假时（本进程内已消失）
/// 不再渲染 logo，页面其余内容保持原位布局。
///
/// `echo` 为回声洞彩蛋状态（UI-006）：只读，用于取当前应显示的前缀与闪烁不透明度。
pub fn view<'a>(
    settings: &SettingsPanel,
    egg: &'a AboutEggState,
    echo: &'a EchoCaveState,
) -> Element<'a> {
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
        // 回声洞彩蛋（UI-006）：位于本页**最后一个文本内容下方**
        .push(space().height(SPACING_CONTENT))
        .push(echo_cave(echo))
        .into()
}

/// 渲染「回声洞」彩蛋文本（UI-006）
///
/// 两层「不抖动」保障：
/// - `width(Fill)`：`MouseArea` 的命中盒 = 子元素布局盒。若直接包裸 `text`，打字初期
///   只有 1~2 个字宽，用户几乎点不中；撑满整行后命中区恒定；
/// - `height(Fixed)`：空串与满串行高一致，打字与闪烁期间页面不上下呼吸。
///
/// `interaction(Pointer)` 请求手型光标（`iced_winit` 映射为 `CursorIcon::Pointer`），
/// 提升「这里能点」的可发现性。
fn echo_cave<'a>(echo: &'a EchoCaveState) -> Element<'a> {
    let line = container(
        text(echo.text())
            .size(TEXT_SIZE_ECHO)
            .style(create_echo_text_style(echo.opacity())),
    )
    .width(Length::Fill)
    .height(Length::Fixed(ECHO_ROW_HEIGHT))
    .align_y(Alignment::Center);

    mouse_area(line)
        .on_press(Message::Settings(crate::Event::EchoCaveClicked))
        .interaction(mouse::Interaction::Pointer)
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
        let echo = EchoCaveState::new();
        // 仅验证构建不 panic 且类型正确（页面结构断言见 ui-settings 集成测试）
        let _element = view(&settings, &egg, &echo);
    }

    #[test]
    fn test_logo_hidden_after_vanish_flag() {
        lumino_ui_core::state::about_egg::mark_logo_vanished();
        let settings = panel();
        let egg = AboutEggState::new();
        let echo = EchoCaveState::new();
        assert!(
            !egg.is_visible_in_page(),
            "进程内已消失时，重开设置面板不应再渲染 logo"
        );
        let _element = view(&settings, &egg, &echo);
        lumino_ui_core::state::about_egg::reset_logo_vanished();
    }

    #[test]
    fn test_about_page_renders_echo_cave_in_all_phases() {
        use lumino_ui_core::state::echo_cave::EchoPhase;

        let settings = panel();
        let egg = AboutEggState::new();

        // ① 未装填（Idle 空串）：行仍需可构建，且高度固定不塌陷
        let mut echo = EchoCaveState::new();
        assert_eq!(echo.phase(), EchoPhase::Idle);
        {
            let _element = view(&settings, &egg, &echo);
        }

        // ② 打字中：前缀非空
        let now = std::time::Instant::now();
        echo.begin_typing(now);
        echo.update(now + std::time::Duration::from_millis(100));
        assert_eq!(echo.phase(), EchoPhase::Typing);
        {
            let _element = view(&settings, &egg, &echo);
        }

        // ③ 闪烁中：alpha 分支被走到
        assert!(echo.on_click(now));
        assert_eq!(echo.phase(), EchoPhase::Blinking);
        {
            let _element = view(&settings, &egg, &echo);
        }
    }
}
