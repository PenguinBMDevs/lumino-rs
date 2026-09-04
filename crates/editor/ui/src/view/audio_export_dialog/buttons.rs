//! 音频导出对话框 - 底部按钮区域
//!
//! 包括：渲染中的进度条状态、非渲染中的关闭/导出按钮。

use iced_widget::{button, column, progress_bar, row, space, text};

use crate::Message;
use crate::message::AudioExportAction;
use crate::state::root_state::AudioExportDialogState;

use crate::view::widgets;

/// 按钮区域（渲染中显示进度，否则显示操作按钮）
pub fn buttons_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    if state.is_rendering {
        render_progress_section(state, palette)
    } else if state.render_completed || state.render_error.is_some() {
        render_finished_section(state, palette)
    } else {
        action_buttons(palette)
    }
}

/// 渲染中：进度条与状态文字 + 暂停/中止
fn render_progress_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let status_text = render_status_text(state, palette);
    let pause_label = if state.is_paused { "继续" } else { "暂停" };
    let pause_action = AudioExportAction::TogglePause;

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
        space().height(12),
        row![
            button(text(pause_label).size(14))
                .on_press(Message::AudioExport(pause_action))
                .padding([8, 24])
                .width(iced_core::Length::Fixed(90.0))
                .style(widgets::dialog_button_style(
                    palette.background.strong.color,
                    palette.background.weak.color,
                    palette.background.neutral.text,
                )),
            space().width(12),
            button(text("中止").size(14))
                .on_press(Message::AudioExport(AudioExportAction::Abort))
                .padding([8, 24])
                .width(iced_core::Length::Fixed(90.0))
                .style(widgets::dialog_button_style(
                    palette.danger.strong.color,
                    palette.danger.base.color,
                    iced_core::Color::WHITE,
                )),
        ]
        .align_y(iced_core::Alignment::Center)
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 渲染完成/失败：显示结果与重置按钮
fn render_finished_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let status_text = render_status_text(state, palette);
    column![
        status_text,
        space().height(12),
        row![
            button(text("重置").size(14))
                .on_press(Message::AudioExport(AudioExportAction::ResetRendering))
                .padding([8, 24])
                .width(iced_core::Length::Fixed(90.0))
                .style(widgets::dialog_button_style(
                    palette.background.strong.color,
                    palette.background.weak.color,
                    palette.background.neutral.text,
                )),
            space().width(12),
            button(text("关闭").size(14))
                .on_press(Message::AudioExport(AudioExportAction::ClosePanel))
                .padding([8, 24])
                .width(iced_core::Length::Fixed(90.0))
                .style(widgets::dialog_button_style(
                    palette.background.strong.color,
                    palette.background.weak.color,
                    palette.background.neutral.text,
                )),
        ]
        .align_y(iced_core::Alignment::Center)
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 渲染状态文字（完成/失败/暂停/进行中）
fn render_status_text<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    if state.is_paused && state.is_rendering {
        text("已暂停")
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            })
            .into()
    } else if state.render_completed {
        text("导出完成")
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.success.strong.color),
            })
            .into()
    } else if let Some(ref err) = state.render_error {
        if err == "已中止" {
            text("已中止")
                .size(14)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                })
                .into()
        } else {
            text(format!("导出失败: {err}"))
                .size(14)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(palette.danger.strong.color),
                })
                .into()
        }
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
