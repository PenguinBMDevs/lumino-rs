use iced_core::Length;
use iced_widget::{button, column, container, pick_list, row, space, text, text_input};
use lumino_extras::i18n::{Language, main_translations};

use crate::message::{CustomPrecisionAction, Message};
use crate::state::root_state::CustomPrecisionDialogState;
use crate::toolbar::DotType;

/// 本地化符点类型包装（支持按语言显示名称）
#[derive(Debug, Clone, Copy)]
struct LocalizedDotType {
    inner: DotType,
    name: &'static str,
}

impl PartialEq for LocalizedDotType {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedDotType {}

impl std::fmt::Display for LocalizedDotType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedDotType {
    fn new(dot_type: DotType, lang: Language) -> Self {
        Self {
            inner: dot_type,
            name: lumino_extras::i18n::dot_type_name(dot_type, lang),
        }
    }
}

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

    // 第一行：三连音数量 + 符点下拉 + 分音符 + "分音符"
    // 当符点类型为（无）时，禁用三连音数量输入框
    let is_tuplet_disabled = state.dot_type == DotType::None;

    // 本地化符点下拉选项
    let dot_options: Vec<LocalizedDotType> = DotType::all()
        .iter()
        .copied()
        .map(|d| LocalizedDotType::new(d, language))
        .collect();
    let current_dot = LocalizedDotType::new(state.dot_type, language);

    let first_row = row![
        // 三连音数量输入框
        container(
            text_input("", &state.tuplet_count)
                .on_input_maybe(if is_tuplet_disabled {
                    None
                } else {
                    Some(|s| Message::CustomPrecision(CustomPrecisionAction::TupletCountChanged(s)))
                })
                .padding([6, 10])
                .width(Length::Fixed(50.0))
        )
        .width(Length::Fixed(50.0))
        .style(input_style),
        space().width(8),
        // 符点类型下拉框
        pick_list(dot_options, Some(current_dot), |ld| {
            Message::CustomPrecision(CustomPrecisionAction::DotTypeChanged(ld.inner))
        })
        .padding([6, 8])
        .width(Length::Fixed(100.0)),
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
    let input_area = column![first_row, space().height(20), second_row,]
        .width(Length::Fixed(320.0))
        .align_x(iced_core::Alignment::Start);

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
