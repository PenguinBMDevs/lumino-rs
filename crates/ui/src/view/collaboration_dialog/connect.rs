use iced_core::{Length, Theme};
use iced_widget::{button, column, row, space, text, text_input};

use crate::message::Message;
use crate::state::root_state::CollaborationDialogState;

pub(super) fn view_connect<'a>(
    state: &'a CollaborationDialogState,
    theme: &'a Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let host_input = text_input("服务器地址", &state.server_host)
        .on_input(Message::CollaborationHostChanged)
        .padding(8)
        .width(Length::Fill);

    let port_input = text_input("端口", &state.server_port)
        .on_input(Message::CollaborationPortChanged)
        .padding(8)
        .width(Length::Fixed(80.0));

    let username_input = text_input("用户", &state.username)
        .on_input(Message::CollaborationUsernameChanged)
        .padding(8)
        .width(Length::Fill);

    let invite_input = text_input("邀请码（可选）", &state.invite_code)
        .on_input(Message::CollaborationInviteCodeChanged)
        .padding(8)
        .width(Length::Fill);

    let (button_text, is_connecting) = if !state.connection_status.is_empty() {
        (state.connection_status.clone(), true)
    } else {
        ("连接".to_string(), false)
    };

    let connect_button = if is_connecting {
        button(text(button_text).size(14))
            .padding([8, 24])
            .style(move |_theme: &Theme, _status| button::Style {
                background: Some(palette.background.weak.color.into()),
                text_color: palette.background.neutral.text,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                snap: false,
                shadow: Default::default(),
            })
    } else {
        button(text(button_text).size(14))
            .on_press(Message::CollaborationConnect {
                host: state.server_host.clone(),
                port: state.server_port.parse().unwrap_or(3000),
                username: state.username.clone(),
                invite_code: if state.invite_code.trim().is_empty() {
                    None
                } else {
                    Some(state.invite_code.clone())
                },
            })
            .padding([8, 24])
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
            })
    };

    column![
        row![host_input, space().width(8), port_input].align_y(iced_core::Alignment::Center),
        space().height(12),
        username_input,
        space().height(12),
        invite_input,
        space().height(16),
        connect_button,
    ]
    .align_x(iced_core::Alignment::Center)
    .into()
}
