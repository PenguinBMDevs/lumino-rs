use iced_core::Theme;
use iced_widget::{button, column, row, space, text};

use crate::message::Message;
use crate::state::root_state::CollaborationDialogState;

pub(super) fn view_in_room<'a>(
    state: &'a CollaborationDialogState,
    theme: &'a Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let room_info = column![
        text(format!("房间: {}", state.room_name))
            .size(16)
            .style(move |_theme: &Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
        space().height(8),
        row![
            text("邀请码: ")
                .size(14)
                .style(move |_theme: &Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                }),
            text(&state.invite_code)
                .size(14)
                .style(move |_theme: &Theme| text::Style {
                    color: Some(palette.primary.base.color),
                }),
        ]
        .align_y(iced_core::Alignment::Center),
    ]
    .align_x(iced_core::Alignment::Center);

    let copy_button = button(text("复制邀请码").size(12))
        .on_press(Message::CollaborationCopyInviteCode)
        .padding([6, 16])
        .style(move |_theme: &Theme, status| {
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
        });

    let disconnect_button = button(text("断开连接").size(14))
        .on_press(Message::CollaborationDisconnect)
        .padding([8, 24])
        .style(move |_theme: &Theme, status| {
            let bg = match status {
                button::Status::Hovered => palette.danger.strong.color,
                _ => palette.danger.base.color,
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
        });

    column![
        room_info,
        space().height(16),
        copy_button,
        space().height(24),
        disconnect_button,
    ]
    .align_x(iced_core::Alignment::Center)
    .into()
}
