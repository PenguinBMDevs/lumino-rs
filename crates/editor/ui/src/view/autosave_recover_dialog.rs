//! 自动保存恢复对话框（DEBT-12）
//!
//! 启动时若检测到上次会话遗留的自动保存，用应用内对话框替代平台原生
//! `MessageDialog`：展示文件路径与修改时间，提供「恢复」/「不恢复」两条路径。
//!
//! 语义与原生版本一致：
//! - 恢复 = 按「未保存工程」载入（Runner 侧清空 source、置 modified）；
//! - 不恢复 = 文件改名保留（`*-declined-*`），下次启动不再询问。

use iced_core::{Border, Length};
use iced_widget::{button, column, container, row, space, text};

use lumino_core::tr;

use crate::message::{AutosaveRecoverAction, Message};
use crate::state::root_state::AutosaveRecoverDialogState;

/// 渲染自动保存恢复对话框
pub fn view_autosave_recover_dialog<'a>(
    state: &'a AutosaveRecoverDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();
    let text_color = palette.background.base.text;
    let secondary = palette.background.strong.text;

    let title = text(tr!("检测到未保存的自动保存"))
        .size(17)
        .style(move |_t: &iced_core::Theme| text::Style {
            color: Some(text_color),
        });

    let path_line = text(state.path.clone())
        .size(13)
        .style(move |_t: &iced_core::Theme| text::Style {
            color: Some(secondary),
        });

    let time_line = text(if state.modified_at.is_empty() {
        tr!("修改时间未知").to_string()
    } else {
        format!("{}：{}", tr!("修改于"), state.modified_at)
    })
    .size(13)
    .style(move |_t: &iced_core::Theme| text::Style {
        color: Some(secondary),
    });

    let hint = text(tr!("恢复不会改动任何已有文件。").to_string())
        .size(13)
        .style(move |_t: &iced_core::Theme| text::Style {
            color: Some(secondary),
        });

    let buttons = row![
        button(text(tr!("恢复自动保存")).size(14))
            .on_press(Message::AutosaveRecover(AutosaveRecoverAction::Restore))
            .padding([8, 20])
            .width(Length::Fixed(130.0))
            .style(move |_t: &iced_core::Theme, status| {
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
        space().width(10),
        button(
            text(tr!("不恢复"))
                .size(14)
                .style(move |_t: &iced_core::Theme| text::Style {
                    color: Some(secondary),
                })
        )
        .on_press(Message::AutosaveRecover(AutosaveRecoverAction::Discard))
        .padding([8, 20])
        .width(Length::Fixed(100.0))
        .style(move |_t: &iced_core::Theme, status| {
            let bg = match status {
                button::Status::Hovered => palette.background.strong.color,
                _ => palette.background.weak.color,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: secondary,
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

    let main_content = column![
        title,
        space().height(10),
        path_line,
        space().height(4),
        time_line,
        space().height(8),
        hint,
        space().height(24),
        buttons,
    ]
    .width(Length::Fill)
    .align_x(iced_core::Alignment::Center);

    let dialog_content = container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
