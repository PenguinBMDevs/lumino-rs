//! 音频导出对话框 - 底部按钮区域
//!
//! 包括：渲染中的进度条状态、非渲染中的关闭/导出按钮。

use iced_widget::{button, column, progress_bar, row, space, text};

use crate::message::{AudioExportAction, Message};
use crate::state::root_state::AudioExportDialogState;

use crate::view::widgets;

/// 按钮区域（渲染中显示进度，否则显示操作按钮）
pub fn buttons_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    if state.is_rendering {
        render_progress_section(state, palette)
    } else {
        action_buttons(palette)
    }
}

/// 渲染中：进度条与状态文字
fn render_progress_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let status_text = render_status_text(state, palette);

    column![
        status_text,
        space().height(8),
        progress_bar(0.0..=1.0, state.render_progress as f32),
        space().height(4),
        text(format!("{:.1}%", state.render_progress * 100.0))
            .size(12)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.strong.text),
            }),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 渲染状态文字（完成/失败/进行中）
fn render_status_text<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    if state.render_completed {
        text("导出完成")
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.success.strong.color),
            })
            .into()
    } else if let Some(ref err) = state.render_error {
        text(format!("导出失败: {err}"))
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.danger.strong.color),
            })
            .into()
    } else {
        text(&state.render_message)
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            })
            .into()
    }
}

/// 非渲染中：关闭/导出按钮
fn action_buttons(palette: &iced_core::theme::palette::Extended) -> crate::Element<'static> {
    row![
        button(text("关闭").size(14))
            .on_press(Message::AudioExport(AudioExportAction::ClosePanel))
            .padding([8, 32])
            .width(iced_core::Length::Fixed(100.0))
            .style(widgets::dialog_button_style(
                palette.background.strong.color,
                palette.background.weak.color,
                palette.background.neutral.text,
            )),
        space().width(12),
        button(text("导出").size(14))
            .on_press(Message::AudioExport(AudioExportAction::Confirm))
            .padding([8, 32])
            .width(iced_core::Length::Fixed(100.0))
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                iced_core::Color::WHITE,
            )),
    ]
    .align_y(iced_core::Alignment::Center)
    .into()
}
