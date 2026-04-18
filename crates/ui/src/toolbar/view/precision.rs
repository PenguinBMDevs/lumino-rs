//! 精度设置区域

use iced_core::Alignment;
use iced_widget::{container, pick_list, row, space, text};

use crate::toolbar::{Event, NotePrecision, RESIZE_HANDLE_HEIGHT};
use crate::{Element, Message, Theme, window};

use super::Toolbar;

pub(super) fn precision_selector<'a>(
    toolbar: &'a Toolbar,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let content_height = toolbar.height - RESIZE_HANDLE_HEIGHT;

    let precision_options: Vec<NotePrecision> = NotePrecision::presets()
        .iter()
        .copied()
        .chain(std::iter::once(NotePrecision::Custom))
        .collect();

    container(
        row![
            text("精度:").size(14),
            space().width(8),
            pick_list(
                precision_options,
                Some(toolbar.note_precision),
                |precision| {
                    if precision == NotePrecision::Custom {
                        Message::OpenCustomPrecisionDialog
                    } else {
                        Event::precision_changed(precision)
                    }
                },
            )
            .placeholder("选择精度")
            .padding([4, 8])
            .width(iced_widget::core::Length::Fixed(120.0)),
        ]
        .align_y(Alignment::Center),
    )
    .height(content_height)
    .align_y(iced_core::alignment::Vertical::Center)
    .padding([0, 16])
    .style(move |_theme: &Theme| {
        container::Style::default().background(palette.background.weakest.color)
    })
    .into()
}
