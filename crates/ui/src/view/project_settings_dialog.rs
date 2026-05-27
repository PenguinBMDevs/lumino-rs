use iced_core::Length;
use iced_widget::{button, column, container, row, space, text, text_input};

use crate::message::Message;
use crate::state::root_state::ProjectSettingsDialogState;

/// 渲染工程设置对话框
pub fn view_project_settings_dialog<'a>(
    state: &'a ProjectSettingsDialogState,
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

    // 标签样式
    let label_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    // 只读文本样式
    let readonly_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    // 标题
    let title = text("工程信息设置")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(label_style);

    // 项目名称
    let title_label = text("项目名称").size(14).style(label_style);
    let title_input = container(
        text_input("输入项目名称（留空显示为'无标题'）", &state.title)
            .on_input(Message::ProjectSettingsTitleChanged)
            .padding([6, 10])
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .style(input_style);

    // BPM 速度
    let tempo_label = text("BPM 速度").size(14).style(label_style);
    let tempo_input = container(
        text_input("120", &state.tempo)
            .on_input(Message::ProjectSettingsTempoChanged)
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
    let copyright_label = text("版权信息").size(14).style(label_style);
    let copyright_input = container(
        text_input("输入版权信息（可选）", &state.copyright)
            .on_input(Message::ProjectSettingsCopyrightChanged)
            .padding([6, 10])
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .style(input_style);

    // 创建日期 (只读)
    let created_label = text("创建日期").size(14).style(label_style);
    let created_value = if state.created_display.is_empty() {
        text("未知").size(14).style(readonly_style)
    } else {
        text(&state.created_display).size(14).style(readonly_style)
    };

    // 累计创作时间 (只读)
    let editing_time_label = text("累计创作时间").size(14).style(label_style);
    let editing_time_value = text(state.format_editing_time())
        .size(14)
        .style(readonly_style);

    // 按钮区域
    let ok_button = button(text("确定").size(14))
        .on_press(Message::ConfirmProjectSettings)
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

    let cancel_button = button(text("取消").size(14))
        .on_press(Message::CloseProjectSettingsDialog)
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

    let buttons = row![cancel_button, space().width(12), ok_button]
        .align_y(iced_core::Alignment::Center);

    // 表单内容
    let form = column![
        title,
        space().height(20),
        title_label,
        title_input,
        space().height(8),
        tempo_label,
        tempo_row,
        space().height(8),
        copyright_label,
        copyright_input,
        space().height(8),
        created_label,
        created_value,
        space().height(8),
        editing_time_label,
        editing_time_value,
        space().height(20),
        buttons,
    ]
    .spacing(4)
    .align_x(iced_core::Alignment::Start)
    .width(Length::Fill);

    let dialog_content = container(form)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
