//! 首次使用提示浮层（DRAW-038）
//!
//! 非模态、右下角；显示即视为已读（Runner 持久化），点击「知道了」关闭。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, space, text};
use lumino_core::tr;
use lumino_extras::i18n::Language;
use lumino_message::HelpAction;
use lumino_ui_core::help::HelpTopic;

use crate::message::Message;
use crate::{Element, window};

/// 渲染当前首次提示；无活动提示时返回 `None`。
pub fn view_help_tip<'a>(
    topic: Option<&'a HelpTopic>,
    window: &'a window::Window,
    language: Language,
) -> Option<Element<'a>> {
    let topic = topic?;
    let palette = window.theme.extended_palette();
    let first_paragraph = topic
        .body_text(language)
        .split("\n\n")
        .next()
        .unwrap_or_default();

    let card = container(
        column![
            text(topic.title_text(language)).size(15),
            space().height(6),
            text(first_paragraph).size(12).width(Length::Fill),
            space().height(10),
            row![
                button(text(tr!("查看帮助")).size(12))
                    .padding([5, 12])
                    .on_press(Message::Core(crate::event::Event::Window(
                        crate::event::window::Event::toggle_help_dialog(),
                    ))),
                space().width(8),
                button(text(tr!("知道了")).size(12))
                    .padding([5, 12])
                    .on_press(Message::Help(HelpAction::DismissTip)),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(2),
    )
    .width(Length::Fixed(360.0))
    .padding(14)
    .style(move |_theme: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: Some(palette.background.weak.text),
        border: iced_core::Border {
            width: 1.0,
            radius: 8.0.into(),
            color: palette.background.strong.color,
        },
        ..Default::default()
    });

    Some(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .padding(16)
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_tip_builds_when_active() {
        let window = window::Window::new("dark");
        let topic = lumino_ui_core::help::topic("curve").expect("curve topic");
        let _ = view_help_tip(Some(topic), &window, Language::ZhCn);
        let _ = view_help_tip(None, &window, Language::EnUs);
    }

    #[test]
    fn test_help_tip_none_when_inactive() {
        let window = window::Window::new("dark");
        assert!(view_help_tip(None, &window, Language::ZhCn).is_none());
    }
}
