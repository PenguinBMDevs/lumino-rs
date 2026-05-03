use iced_core::Length;
use iced_widget::{button, column, container, row, space, text};

use crate::message::Message;
use crate::state::root_state::LoadConfirmDialogState;

/// 渲染加载确认对话框
pub fn view_load_confirm_dialog<'a>(
    state: &'a LoadConfirmDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    // 文件信息区域
    let file_info = column![
        text(format!("文件: {}", state.file_name))
            .size(14)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
        space().height(6),
        text(format!("大小: {:.1} MB", state.size_mb))
            .size(14)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
        space().height(6),
        text(format!("路径: {}", state.file_path))
            .size(12)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.background.weak.text),
            }),
        space().height(16),
        text("此文件较大，加载可能需要一些时间。")
            .size(13)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.background.weak.text),
            }),
    ]
    .width(Length::Fill);

    // 按钮区域
    let buttons = row![
        button(
            text("取消")
                .size(14)
                .style(move |_t: &iced_core::Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                }),
        )
        .on_press(Message::CloseLoadConfirmDialog)
        .padding([8, 32])
        .width(Length::Fixed(100.0))
        .style(move |_t: &iced_core::Theme, status| {
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
        space().width(12),
        button(text("加载").size(14))
            .on_press(Message::ConfirmLoadConfirm)
            .padding([8, 32])
            .width(Length::Fixed(100.0))
            .style(move |_t: &iced_core::Theme, status| {
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
    ]
    .align_y(iced_core::Alignment::Center);

    // 主内容
    let main_content = column![file_info, space().height(24), buttons,]
        .width(Length::Fill)
        .align_x(iced_core::Alignment::Start);

    let dialog_content = container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
