//! 保存确认对话框视图
//!
//! 关闭工程 / 打开另一个工程 / 退出软件时，若当前工程存在未保存的更改，
//! 弹出此对话框让用户决定：保存当前工程、放弃更改直接关闭、或取消关闭操作。

use iced_core::Length;
use iced_widget::{button, column, container, row, space, text};

use crate::message::{Message, SaveConfirmAction};
use crate::state::root_state::SaveConfirmDialogState;

/// 渲染保存确认对话框
pub fn view_save_confirm_dialog<'a>(
    _state: &'a SaveConfirmDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    // 居中提示文字
    let prompt = text("是否保留未保存的更改？")
        .size(16)
        .style(move |_t: &iced_core::Theme| text::Style {
            color: Some(palette.background.neutral.text),
        });

    // 按钮区域：保存（主）/ 关闭（次）/ 取消（次）
    let buttons = row![
        button(text("保存").size(14))
            .on_press(Message::SaveConfirm(SaveConfirmAction::Save))
            .padding([8, 28])
            .width(Length::Fixed(90.0))
            .style(move |_t: &iced_core::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => palette.primary.strong.color,
                    _ => palette.primary.base.color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: iced_core::Color::WHITE,
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    snap: false,
                    shadow: Default::default(),
                }
            }),
        space().width(10),
        button(
            text("关闭")
                .size(14)
                .style(move |_t: &iced_core::Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                }),
        )
        .on_press(Message::SaveConfirm(SaveConfirmAction::Discard))
        .padding([8, 28])
        .width(Length::Fixed(90.0))
        .style(move |_t: &iced_core::Theme, status| {
            let bg = match status {
                button::Status::Hovered => palette.background.strong.color,
                _ => palette.background.weak.color,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: palette.background.neutral.text,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                snap: false,
                shadow: Default::default(),
            }
        }),
        space().width(10),
        button(
            text("取消")
                .size(14)
                .style(move |_t: &iced_core::Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                }),
        )
        .on_press(Message::SaveConfirm(SaveConfirmAction::Cancel))
        .padding([8, 28])
        .width(Length::Fixed(90.0))
        .style(move |_t: &iced_core::Theme, status| {
            let bg = match status {
                button::Status::Hovered => palette.background.strong.color,
                _ => palette.background.weak.color,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: palette.background.neutral.text,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                snap: false,
                shadow: Default::default(),
            }
        }),
    ]
    .align_y(iced_core::Alignment::Center);

    // 主内容：提示文字 + 按钮，整体居中
    let main_content = column![space().height(12), prompt, space().height(24), buttons,]
        .width(Length::Fill)
        .align_x(iced_core::Alignment::Center);

    let dialog_content = container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
