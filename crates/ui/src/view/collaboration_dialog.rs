use iced_core::Length;
use iced_widget::{button, column, container, row, space, text, text_input};

use crate::message::Message;
use crate::state::root_state::{CollaborationDialogState, CollaborationViewState};

/// 渲染协作对话框
pub fn view_collaboration_dialog<'a>(
    state: &'a CollaborationDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    // 标题
    let title = text("多人协作")
        .size(20)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(palette.background.neutral.text),
        });

    // 根据当前视图状态显示不同内容
    let content: crate::Element<'_> = match state.view_state {
        CollaborationViewState::Connect => {
            // 连接服务器界面
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

            let connect_button = button(text("连接").size(14))
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
                .style(move |_theme: &iced_core::Theme, status| {
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

            column![
                row![host_input, space().width(8), port_input]
                    .align_y(iced_core::Alignment::Center),
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
        CollaborationViewState::RoomActions => {
            // 创建/加入房间界面
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
                .style(move |_theme: &iced_core::Theme, status| {
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
                .style(move |_theme: &iced_core::Theme| text::Style {
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
                .style(move |_theme: &iced_core::Theme, status| {
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
        CollaborationViewState::InRoom => {
            // 在房间内界面
            let room_info = column![
                text(format!("房间: {}", state.room_name)).size(16).style(
                    move |_theme: &iced_core::Theme| text::Style {
                        color: Some(palette.background.neutral.text),
                    }
                ),
                space().height(8),
                row![
                    text("邀请码: ")
                        .size(14)
                        .style(move |_theme: &iced_core::Theme| text::Style {
                            color: Some(palette.background.neutral.text),
                        }),
                    text(&state.invite_code)
                        .size(14)
                        .style(move |_theme: &iced_core::Theme| text::Style {
                            color: Some(palette.primary.base.color),
                        }),
                ]
                .align_y(iced_core::Alignment::Center),
            ]
            .align_x(iced_core::Alignment::Center);

            let copy_button = button(text("复制邀请码").size(12))
                .on_press(Message::CollaborationCopyInviteCode)
                .padding([6, 16])
                .style(move |_theme: &iced_core::Theme, status| {
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
                .style(move |_theme: &iced_core::Theme, status| {
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
    };

    // 关闭按钮
    let close_button = button(text("关闭").size(12))
        .on_press(Message::CloseCollaborationDialog)
        .padding([6, 16])
        .style(move |_theme: &iced_core::Theme, status| {
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

    let dialog_content = column![
        row![title, space().width(Length::Fill), close_button]
            .align_y(iced_core::Alignment::Center),
        space().height(20),
        content,
    ]
    .align_x(iced_core::Alignment::Center);

    container(dialog_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        })
        .into()
}
