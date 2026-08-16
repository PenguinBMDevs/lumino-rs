use iced_core::Length;
use iced_widget::{button, column, container, row, space, text, text_input};

use crate::message::{Message, SpeedChangeAction};
use crate::state::root_state::SpeedChangeDialogState;

/// 渲染音符变速对话框
pub fn view_speed_change_dialog<'a>(
    state: &'a SpeedChangeDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    // 输入框样式
    let input_style = move |_theme: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    };

    // 标题
    let title = text("变速")
        .size(18)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(palette.background.neutral.text),
        });

    // 倍率输入行
    let factor_row = row![
        text("倍率(S):")
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
        space().width(12),
        container(
            text_input("如: 0.5 或 1/3", &state.factor_input)
                .on_input(|v| Message::SpeedChange(SpeedChangeAction::FactorChanged(v)))
                .padding([6, 10])
                .width(Length::Fixed(120.0))
        )
        .width(Length::Fixed(120.0))
        .style(input_style),
    ]
    .align_y(iced_core::Alignment::Center);

    // 提示文字
    let hint =
        text("支持小数 (0.5) 或分数 (1/3)")
            .size(12)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.strong.text),
            });

    // 按钮区域
    let buttons = row![
        button(text("确定").size(14))
            .on_press(Message::SpeedChange(SpeedChangeAction::Confirm))
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
            }),
        space().width(12),
        button(text("取消").size(14))
            .on_press(Message::SpeedChange(SpeedChangeAction::CloseDialog))
            .padding([8, 24])
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
            }),
    ]
    .align_y(iced_core::Alignment::Center);

    // 主内容
    let content = column![
        title,
        space().height(20),
        factor_row,
        space().height(8),
        hint,
        space().height(20),
        buttons,
    ]
    .align_x(iced_core::Alignment::Start)
    .spacing(4);

    let dialog_content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
