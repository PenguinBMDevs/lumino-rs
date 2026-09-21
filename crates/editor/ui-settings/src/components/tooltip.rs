//! 设置项悬浮说明
//!
//! 说明文字不再常驻显示：tooltip 只挂在设置项的**文字**上（控件本身不触发），
//! 悬停文字约 500ms 后显示提示；鼠标光标在悬停文字时**立即**变为系统「帮助」光标
//! （Windows/macOS/Linux 的箭头+问号），与提示延迟无关。

use iced_core::mouse::Interaction;
use iced_core::time::Duration;
use iced_core::{Alignment, Length};
use iced_widget::{container, mouse_area, tooltip};

use lumino_ui_core::{Element, Message, widget::with_tooltip};

/// 设置项说明的悬停延迟：避免鼠标扫过时提示闪烁。
const SETTING_TOOLTIP_DELAY: Duration = Duration::from_millis(500);

/// 给整行设置项挂悬浮说明（悬停区域 = 整行宽度，内容左对齐）。
///
/// 悬停 `SETTING_TOOLTIP_DELAY` 后显示，移出即消失；提示右置并自动贴合窗口边缘。
/// 注意：整行包裹会连控件一起触发，通常应改用 [`with_setting_tooltip_inline`]，
/// 只把提示挂在文字标签上。
pub fn with_setting_tooltip<'a>(
    content: impl Into<Element<'a>>,
    tooltip_text: &'a str,
) -> Element<'a> {
    let content = container(content)
        .width(Length::Fill)
        .align_x(Alignment::Start);

    with_tooltip(content, tooltip_text, tooltip::Position::Right)
        .delay(SETTING_TOOLTIP_DELAY)
        .into()
}

/// 给设置项的标签文字挂悬浮说明（推荐用法）。
///
/// - 悬停区域 = 文字本身；控件（对钩框/滑块/下拉框）不触发提示；
/// - 悬停文字时鼠标**立即**变为「帮助」光标，提示延迟不变；
/// - 提示右置、相对文字固定，移出即消失。
pub fn with_setting_tooltip_inline<'a>(
    content: impl Into<Element<'a>>,
    tooltip_text: &'a str,
) -> Element<'a> {
    let area = mouse_area(content).interaction(Interaction::Help);

    with_tooltip(area, tooltip_text, tooltip::Position::Right)
        .delay(SETTING_TOOLTIP_DELAY)
        .into()
}

/// 同 [`with_setting_tooltip_inline`]，但标签文字可点击（用于替代对钩框自带的标签点击）。
///
/// `on_press` 传入点击文字时应发送的消息（通常是对钩切换事件）。
pub fn with_setting_tooltip_inline_action<'a>(
    content: impl Into<Element<'a>>,
    tooltip_text: &'a str,
    on_press: Message,
) -> Element<'a> {
    let area = mouse_area(content)
        .interaction(Interaction::Help)
        .on_press(on_press);

    with_tooltip(area, tooltip_text, tooltip::Position::Right)
        .delay(SETTING_TOOLTIP_DELAY)
        .into()
}
