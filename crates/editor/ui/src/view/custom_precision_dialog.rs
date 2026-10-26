use iced_core::Length;
use iced_widget::{button, column, container, row, space, text, text_input};
use lumino_core::tr;
use lumino_extras::i18n::{Language, main_translations};

use crate::message::{CustomPrecisionAction, Message};
use crate::state::root_state::CustomPrecisionDialogState;
use crate::toolbar::DotType;

/// 渲染自定义精度对话框
pub fn view_custom_precision_dialog<'a>(
    state: &'a CustomPrecisionDialogState,
    theme: &'a iced_core::Theme,
    language: Language,
) -> crate::Element<'a> {
    let t = main_translations(language);
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

    // 第一行：三连音数 + 分音符；附点改为独立按钮行，避免下拉层必须
    // 先让别的控件拿到焦点才能点开。
    // 三连音数始终可编辑：DRAW-037 中 div = 三连音数 × 除数。
    let first_row = row![
        // 三连音数量输入框
        container(
            text_input("", &state.tuplet_count)
                .on_input(
                    |s| Message::CustomPrecision(CustomPrecisionAction::TupletCountChanged(s))
                )
                .padding([6, 10])
                .width(Length::Fixed(50.0))
        )
        .width(Length::Fixed(50.0))
        .style(input_style),
        space().width(8),
        // 分音符值输入框
        container(
            text_input("", &state.note_value)
                .on_input(|s| Message::CustomPrecision(CustomPrecisionAction::NoteValueChanged(s)))
                .padding([6, 10])
                .width(Length::Fixed(50.0))
        )
        .width(Length::Fixed(50.0))
        .style(input_style),
        space().width(8),
        // "分音符" 标签
        text(t.precision_note_label)
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
    ]
    .align_y(iced_core::Alignment::Center);

    // 附点：分段按钮，点击即生效；不依赖 pick_list 的浮层焦点。
    let mut dot_row = row![
        text(tr!("附点"))
            .size(13)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
    ]
    .spacing(6)
    .align_y(iced_core::Alignment::Center);
    for dot in DotType::all().iter().copied() {
        let selected = state.dot_type == dot;
        dot_row = dot_row.push(
            button(text(lumino_extras::i18n::dot_type_name(dot, language)).size(11))
                .on_press(Message::CustomPrecision(
                    CustomPrecisionAction::DotTypeChanged(dot),
                ))
                .padding([4, 7])
                .style(move |theme: &iced_core::Theme, status| {
                    let palette = theme.extended_palette();
                    let background = if selected {
                        palette.primary.weak.color
                    } else if status == button::Status::Hovered {
                        palette.background.strong.color
                    } else {
                        palette.background.weak.color
                    };
                    button::Style {
                        background: Some(background.into()),
                        text_color: if selected {
                            palette.primary.weak.text
                        } else {
                            palette.background.neutral.text
                        },
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 1.0,
                            color: palette.background.strong.color,
                        },
                        ..Default::default()
                    }
                }),
        );
    }

    // 第二行："除以" + 除数输入框
    let second_row = row![
        text(t.precision_divide_by)
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
        space().width(50),
        container(
            text_input("", &state.divisor)
                .on_input(|s| Message::CustomPrecision(CustomPrecisionAction::DivisorChanged(s)))
                .padding([6, 10])
                .width(Length::Fixed(50.0))
        )
        .width(Length::Fixed(50.0))
        .style(input_style),
    ]
    .align_y(iced_core::Alignment::Center);

    // 左侧输入区域
    let mut input_area = column![
        first_row,
        space().height(12),
        dot_row,
        space().height(12),
        second_row,
    ];
    if let Some(error) = &state.error {
        input_area = input_area.push(space().height(6)).push(
            text(error.clone())
                .size(11)
                .width(Length::Fixed(320.0))
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(palette.danger.base.color),
                }),
        );
    }
    let input_area = input_area
        .width(Length::Fixed(320.0))
        .align_x(iced_core::Alignment::Start);
    // 附点按钮行独立于输入框，点击不再依赖 pick_list 的浮层焦点。

    // 右侧按钮区域（垂直排列）
    let buttons = column![
        button(text(t.precision_ok).size(14))
            .on_press(Message::CustomPrecision(CustomPrecisionAction::Confirm))
            .padding([8, 32])
            .width(Length::Fixed(100.0))
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
        space().height(12),
        button(text(t.precision_cancel).size(14))
            .on_press(Message::CustomPrecision(CustomPrecisionAction::CloseDialog))
            .padding([8, 32])
            .width(Length::Fixed(100.0))
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
                    shadow: Default::default(),
                    snap: false,
                }
            }),
    ]
    .align_x(iced_core::Alignment::Center);

    // 主内容区域：左侧输入 + 右侧按钮
    let main_content = row![input_area, space().width(Length::Fixed(20.0)), buttons,]
        .align_y(iced_core::Alignment::Center);

    let dialog_content = container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    #[test]
    fn test_custom_precision_dialog_builds_with_dot_buttons() {
        let state = CustomPrecisionDialogState::default();
        for theme in [Theme::Dark, Theme::Light] {
            let _ = view_custom_precision_dialog(&state, &theme, Language::ZhCn);
            let _ = view_custom_precision_dialog(&state, &theme, Language::EnUs);
        }
    }
}
