use iced_core::{Border, Length};
use iced_widget::{button, column, container, row, space, text, text_input};

use crate::message::{BatchEditAction, BatchEditField, Message};
use crate::state::root_state::BatchEditDialogState;

const SECTION_RADIUS: f32 = 16.0;
const SECTION_PADDING: f32 = 16.0;
const SECTION_SPACING: f32 = 12.0;
const INPUT_WIDTH: f32 = 120.0;
const LABEL_WIDTH: f32 = 110.0;

/// 渲染批量编辑对话框
pub fn view_batch_edit_dialog<'a>(
    state: &'a BatchEditDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let section_style = move |_theme: &iced_core::Theme| container::Style {
        background: Some(palette.background.base.color.into()),
        border: Border {
            radius: SECTION_RADIUS.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    };

    let input_style = move |_theme: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        border: Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    };

    let text_color = palette.background.neutral.text;
    let hint_color = palette.background.strong.text;

    let title = text("批量编辑")
        .size(18)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(text_color),
        });

    let input_row =
        |label: &'a str, placeholder: &'a str, value: &'a str, on_input: fn(String) -> Message| {
            row![
                container(
                    text(label)
                        .size(14)
                        .style(move |_theme: &iced_core::Theme| {
                            text::Style {
                                color: Some(text_color),
                            }
                        })
                )
                .width(Length::Fixed(LABEL_WIDTH)),
                container(
                    text_input(placeholder, value)
                        .on_input(on_input)
                        .padding([6, 10])
                        .width(Length::Fixed(INPUT_WIDTH))
                )
                .width(Length::Fixed(INPUT_WIDTH))
                .style(input_style),
            ]
            .spacing(8)
            .align_y(iced_core::Alignment::Center)
        };

    let settings_section = container(
        column![
            text("设置")
                .size(16)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(text_color),
                }),
            space().height(8),
            input_row("音符力度:", "如: +20", &state.velocity_input, |v| {
                Message::BatchEdit(BatchEditAction::InputChanged(BatchEditField::Velocity, v))
            }),
            input_row("音符长度:", "如: *1.5", &state.gate_input, |v| {
                Message::BatchEdit(BatchEditAction::InputChanged(BatchEditField::Gate, v))
            }),
            input_row("音符key位置:", "如: 60", &state.key_input, |v| {
                Message::BatchEdit(BatchEditAction::InputChanged(BatchEditField::Key, v))
            }),
            input_row("音符tick位置:", "如: 1000", &state.tick_input, |v| {
                Message::BatchEdit(BatchEditAction::InputChanged(BatchEditField::Tick, v))
            }),
        ]
        .spacing(SECTION_SPACING)
        .align_x(iced_core::Alignment::Start),
    )
    .padding(SECTION_PADDING)
    .width(Length::Fill)
    .style(section_style);

    let format_section = container(
        column![
            text("格式")
                .size(16)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(text_color),
                }),
            space().height(8),
            text("支持以下运算符号:")
                .size(13)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(hint_color)
                }),
            text("+N  增加 N        -N  减少 N").size(12).style(
                move |_theme: &iced_core::Theme| text::Style {
                    color: Some(hint_color)
                }
            ),
            text("*N  乘以 N        /N  除以 N").size(12).style(
                move |_theme: &iced_core::Theme| text::Style {
                    color: Some(hint_color)
                }
            ),
            text("无符号数字表示直接设置为该值").size(12).style(
                move |_theme: &iced_core::Theme| text::Style {
                    color: Some(hint_color)
                }
            ),
        ]
        .spacing(6)
        .align_x(iced_core::Alignment::Start),
    )
    .padding(SECTION_PADDING)
    .width(Length::Fill)
    .style(section_style);

    let buttons = row![
        button(text("确定").size(14))
            .on_press(Message::BatchEdit(BatchEditAction::Confirm))
            .padding([8, 24])
            .style(move |_theme: &iced_core::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => palette.primary.strong.color,
                    _ => palette.primary.base.color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: iced_core::Color::WHITE,
                    border: Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    snap: false,
                    shadow: Default::default(),
                }
            }),
        space().width(12),
        button(text("取消").size(14))
            .on_press(Message::BatchEdit(BatchEditAction::CloseDialog))
            .padding([8, 24])
            .style(move |_theme: &iced_core::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => palette.background.strong.color,
                    _ => palette.background.weak.color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: palette.background.neutral.text,
                    border: Border {
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

    let content = column![
        title,
        space().height(16),
        settings_section,
        space().height(12),
        format_section,
        space().height(20),
        row![space().width(Length::Fill), buttons].align_y(iced_core::Alignment::Center),
    ]
    .spacing(4)
    .align_x(iced_core::Alignment::Start)
    .width(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        })
        .into()
}
