//! Help 窗口（DRAW-038）
//!
//! 左侧分区 + 话题列表，右侧正文，顶部搜索；底部「全部重现」清空首次提示。
//! 窗口为独立 OS 窗口，沿用现有 DialogType 体系。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, scrollable, space, text, text_input};
use lumino_core::tr;
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::HelpAction;
use lumino_ui_core::color::contrast_text_color;
use lumino_ui_core::help::{self, HelpDialogState, HelpSection};

use crate::Element;
use crate::message::Message;

/// 渲染 Help 窗口
pub fn view_help_dialog<'a>(
    state: &'a HelpDialogState,
    theme: &'a iced_core::Theme,
    language: Language,
) -> Element<'a> {
    let t = main_translations(language);
    let results = help::search(&state.query, language);
    let selected = results
        .get(state.selected)
        .copied()
        .or_else(|| results.first().copied());

    let search = text_input(tr!("搜索帮助..."), &state.query)
        .on_input(|query| Message::Help(HelpAction::QueryChanged(query)))
        .padding([7, 10])
        .width(Length::Fill);

    let mut list = column![].spacing(3);
    if results.is_empty() {
        list = list.push(text(tr!("没有匹配的话题")).size(12).width(Length::Fill));
    } else {
        for section in HelpSection::ALL {
            let items: Vec<(usize, &lumino_ui_core::help::HelpTopic)> = results
                .iter()
                .enumerate()
                .filter(|(_, topic)| topic.section == *section)
                .map(|(index, topic)| (index, *topic))
                .collect();
            if items.is_empty() {
                continue;
            }
            list = list.push(text(section.title(language)).size(11).width(Length::Fill));
            for (index, topic) in items {
                let active = selected.is_some_and(|s| s.id == topic.id);
                let label = text(topic.title_text(language)).size(13);
                let button = button(label)
                    .padding([6, 10])
                    .width(Length::Fill)
                    .on_press(Message::Help(HelpAction::TopicSelected(index)));
                list = list.push(if active {
                    button.style(|theme: &iced_core::Theme, _status| button::Style {
                        background: Some(theme.extended_palette().primary.weak.color.into()),
                        text_color: theme.extended_palette().primary.weak.text,
                        border: iced_core::Border::default().rounded(4),
                        ..Default::default()
                    })
                } else {
                    button.style(|theme: &iced_core::Theme, _status| button::Style {
                        background: None,
                        text_color: theme.palette().text,
                        border: iced_core::Border::default().rounded(4),
                        ..Default::default()
                    })
                });
            }
        }
    }

    let body = if let Some(topic) = selected {
        let mut content = column![
            text(topic.title_text(language)).size(22),
            space().height(6),
            text(topic.tip_text(language)).size(12),
            space().height(14),
        ]
        .spacing(2);
        for paragraph in topic.body_text(language).split("\n\n") {
            content = content.push(text(paragraph).size(14).width(Length::Fill));
            content = content.push(space().height(10));
        }
        scrollable(content.padding([4, 12])).height(Length::Fill)
    } else {
        scrollable(text(tr!("没有匹配的话题")).size(14).width(Length::Fill)).height(Length::Fill)
    };

    let content = column![
        text(tr!("帮助")).size(20),
        space().height(10),
        search,
        space().height(12),
        row![
            container(scrollable(list).height(Length::Fill))
                .width(Length::Fixed(240.0))
                .height(Length::Fill),
            space().width(18),
            container(body).width(Length::Fill).height(Length::Fill),
        ]
        .height(Length::Fill),
        space().height(10),
        row![
            button(text(tr!("全部重现")).size(13))
                .padding([6, 14])
                .on_press(Message::Help(HelpAction::ResetTips)),
            space().width(Length::Fill),
            button(text(t.file_close).size(13))
                .padding([6, 16])
                .on_press(Message::Help(HelpAction::Close)),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(2)
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill);

    let background = theme.palette().background;
    let text_color = contrast_text_color(background);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(iced_core::Background::Color(background)),
            text_color: Some(text_color),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    #[test]
    fn test_help_dialog_builds() {
        let state = HelpDialogState::default();
        let _ = view_help_dialog(&state, &Theme::Dark, Language::ZhCn);
        let state = HelpDialogState {
            query: "shortcut".into(),
            selected: 0,
        };
        let _ = view_help_dialog(&state, &Theme::Light, Language::EnUs);
    }
}
