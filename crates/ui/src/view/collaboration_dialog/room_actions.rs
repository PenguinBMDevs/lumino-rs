use iced_core::{Length, Theme};
use iced_widget::{button, column, space, text, text_input};

use crate::message::Message;
use crate::state::root_state::CollaborationDialogState;

pub(super) fn view_room_actions<'a>(
    state: &'a CollaborationDialogState,
    theme: &'a Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let room_name_input = text_input("房间名称", &state.room_name)
        .on_input(Message::CollaborationRoomNameChanged)
        .padding(8)
        .width(Length::Fill);

    let create_button = button(text("创建房间").size(14))
        .on_press(Message::CollaborationCreateRoom {
            name: state.room_name.clone(),
        })
        .padding([8, 24])
        .width(Length::Fill)
        .style(move |_theme: &Theme, status| {
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
        });

    let or_text = text("- 或 -")
        .size(12)
        .style(move |_theme: &Theme| text::Style {
            color: Some(palette.background.neutral.text),
        });

    let invite_input = text_input("邀请码", &state.invite_code)
        .on_input(Message::CollaborationInviteCodeChanged)
        .padding(8)
        .width(Length::Fill);

    let join_button = button(text("加入房间").size(14))
        .on_press(Message::CollaborationJoinRoom {
            invite_code: state.invite_code.clone(),
        })
        .padding([8, 24])
        .width(Length::Fill)
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

    column![
        room_name_input,
        space().height(8),
        create_button,
        space().height(16),
        or_text,
        space().height(16),
        invite_input,
        space().height(8),
        join_button,
    ]
    .align_x(iced_core::Alignment::Center)
    .into()
}
