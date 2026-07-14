use iced_core::Length;
use iced_widget::{column, container, progress_bar, space, text};

use crate::state::root_state::ExportProgressDialogState;

/// 渲染音频导出进度对话框
pub fn view_export_progress_dialog<'a>(
    state: &'a ExportProgressDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    // 标题
    let title = text("音频导出")
        .size(18)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(palette.background.neutral.text),
        });

    // 状态消息
    let message = if let Some(error) = &state.error {
        text(error)
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.danger.strong.color),
            })
    } else if state.is_completed {
        text("导出完成")
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.success.strong.color),
            })
    } else {
        text(&state.message)
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            })
    };

    // 进度条
    let progress = progress_bar(0.0..=1.0, state.progress as f32);

    // 进度百分比
    let percentage = text(format!("{:.1}%", state.progress * 100.0))
        .size(12)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(palette.background.strong.text),
        });

    // 主内容
    let content = column![
        title,
        space().height(20),
        message,
        space().height(16),
        progress,
        space().height(8),
        percentage,
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
