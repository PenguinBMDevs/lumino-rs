use iced_widget::{column, space, text};

use crate::state::root_state::CollaborationDialogState;

pub(super) fn view_connecting<'a>(
    _state: &'a CollaborationDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let connecting_text =
        text("正在连接服务器...")
            .size(16)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.primary.base.color),
            });

    column![
        connecting_text,
        space().height(16),
        text("请稍候")
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
    ]
    .align_x(iced_core::Alignment::Center)
    .into()
}
