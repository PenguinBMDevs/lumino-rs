use iced_core::Length;
use iced_widget::{button, column, container, row, scrollable, space, text, text_input};
use lumino_core::i18n::{Language, main_translations};

use crate::message::{Message, ProjectSettingsAction};
use crate::state::root_state::ProjectSettingsDialogState;

/// 渲染工程设置对话框
pub fn view_project_settings_dialog<'a>(
    state: &'a ProjectSettingsDialogState,
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

    // 标签样式
    let label_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    // 只读文本样式
    let readonly_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    // 标题
    let title = text(t.project_title)
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(label_style);

    // 项目名称
    let title_label = text(t.project_name_label).size(14).style(label_style);
    let title_input = container(
        text_input(t.project_name_placeholder, &state.title)
            .on_input(|s| Message::ProjectSettings(ProjectSettingsAction::TitleChanged(s)))
            .padding([6, 10])
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .style(input_style);

    // BPM 速度
    let tempo_label = text(t.project_bpm_label).size(14).style(label_style);
    let tempo_input = container(
        text_input("120", &state.tempo)
            .on_input(|s| Message::ProjectSettings(ProjectSettingsAction::TempoChanged(s)))
            .padding([6, 10])
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .style(input_style);
    let tempo_unit = text("BPM")
        .size(14)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(palette.background.strong.color),
        });

    let tempo_row = row![tempo_input, space().width(8), tempo_unit]
        .align_y(iced_core::Alignment::Center)
        .width(Length::Fill);

    // 版权信息
    let copyright_label = text(t.project_copyright_label).size(14).style(label_style);
    let copyright_input = container(
        text_input(t.project_copyright_placeholder, &state.copyright)
            .on_input(|s| Message::ProjectSettings(ProjectSettingsAction::CopyrightChanged(s)))
            .padding([6, 10])
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .style(input_style);

    // 拍号（单拍号编辑，tick 固定为 0）
    let time_signature_label = text("拍号").size(14).style(label_style);
    let numerator_input = container(
        text_input("4", &state.time_signature_numerator)
            .on_input(|s| {
                Message::ProjectSettings(ProjectSettingsAction::TimeSignatureNumeratorChanged(s))
            })
            .padding([6, 10])
            .width(Length::Fixed(60.0)),
    )
    .style(input_style);
    let slash = text("/").size(14).style(label_style);
    let denominator_input = container(
        text_input("4", &state.time_signature_denominator)
            .on_input(|s| {
                Message::ProjectSettings(ProjectSettingsAction::TimeSignatureDenominatorChanged(s))
            })
            .padding([6, 10])
            .width(Length::Fixed(60.0)),
    )
    .style(input_style);
    let time_signature_row = row![
        numerator_input,
        space().width(4),
        slash,
        space().width(4),
        denominator_input
    ]
    .align_y(iced_core::Alignment::Center);

    // 创建日期 (只读)
    let created_label = text(t.project_created_label).size(14).style(label_style);
    let created_value = if state.created_display.is_empty() {
        text(t.project_unknown).size(14).style(readonly_style)
    } else {
        text(&state.created_display).size(14).style(readonly_style)
    };

    // 累计创作时间 (只读)
    let editing_time_label = text(t.project_editing_time_label)
        .size(14)
        .style(label_style);
    let editing_time_value = text(state.format_editing_time())
        .size(14)
        .style(readonly_style);

    // 按钮区域
    let ok_button = button(text(t.project_ok).size(14))
        .on_press(Message::ProjectSettings(ProjectSettingsAction::Confirm))
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
        });

    let cancel_button = button(text(t.project_cancel).size(14))
        .on_press(Message::ProjectSettings(ProjectSettingsAction::CloseDialog))
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
        });

    let buttons =
        row![cancel_button, space().width(12), ok_button].align_y(iced_core::Alignment::Center);

    // 表单内容
    let form = column![
        title,
        space().height(20),
        title_label,
        title_input,
        space().height(12),
        tempo_label,
        tempo_row,
        space().height(12),
        copyright_label,
        copyright_input,
        space().height(12),
        time_signature_label,
        time_signature_row,
        space().height(12),
        created_label,
        created_value,
        space().height(12),
        editing_time_label,
        editing_time_value,
        space().height(24),
        buttons,
    ]
    .spacing(4)
    .align_x(iced_core::Alignment::Start)
    .width(Length::Fill);

    // 使用 scrollable 包裹以处理潜在的溢出
    let scrollable_content = scrollable(form).width(Length::Fill).height(Length::Fill);

    let dialog_content = container(scrollable_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
