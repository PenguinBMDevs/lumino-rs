//! 视频导出对话框共享布局辅助函数
//!
//! 提供 pick_list 选择行、预览区域、MIDI 源、输出路径、按钮栏等可复用 UI 组件。

use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, image, pick_list, row, space, text, text_input};

use crate::message::{Message, VideoExportAction};
use crate::view::widgets;

use super::state::VideoExportDialogState;

/// pick_list 选择行
pub fn pick_list_row<'a, T: 'a + Clone + ToString + PartialEq>(
    label: &'a str,
    label_width: f32,
    options: Vec<T>,
    selected: Option<T>,
    on_selected: impl Fn(T) -> Message + 'a,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };
    row![
        text(label).size(14).style(label_style).width(label_width),
        pick_list(options, selected, on_selected).width(Length::Fixed(200.0)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// 标题
pub fn title_section<'a>(palette: &'a iced_core::theme::palette::Extended) -> crate::Element<'a> {
    text("视频导出")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(widgets::dialog_label_style(palette))
        .into()
}

/// MIDI 数据源区域
pub fn midi_source_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let hint = if state.midi_path.is_empty() {
        "优先使用当前工程的 MIDI 数据"
    } else {
        "使用指定 MIDI 文件流式读取"
    };

    column![
        text("MIDI 数据源")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        row![
            container(
                text(&state.midi_path)
                    .size(12)
                    .style(widgets::dialog_muted_text_style(palette))
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::VideoExport(VideoExportAction::BrowseMidi))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(4),
        text(hint)
            .size(12)
            .style(widgets::dialog_muted_text_style(palette)),
    ]
    .width(Length::Fill)
    .into()
}

/// 输出路径区域
pub fn output_path_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("导出位置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        row![
            container(
                text_input("选择输出路径...", &state.output_path)
                    .on_input(|v| Message::VideoExport(VideoExportAction::OutputPathChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::VideoExport(VideoExportAction::BrowseOutput))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .into()
}

/// 关闭/导出按钮
pub fn buttons_section(palette: &iced_core::theme::palette::Extended) -> crate::Element<'static> {
    row![
        button(text("关闭").size(14))
            .on_press(Message::VideoExport(VideoExportAction::ClosePanel))
            .padding([8, 32])
            .width(Length::Fixed(100.0))
            .style(widgets::dialog_button_style(
                palette.background.strong.color,
                palette.background.weak.color,
                palette.background.neutral.text,
            )),
        space().width(12),
        button(text("开始导出").size(14))
            .on_press(Message::VideoExport(VideoExportAction::StartExport))
            .padding([8, 32])
            .width(Length::Fixed(120.0))
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                Color::WHITE,
            )),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// 预览区域（有缓存图片时）
pub fn preview_area<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    if let Some(ref handle) = state.cached_image_handle {
        let preview_max_w = 480.0;
        let preview_max_h = 240.0;
        let img_w = state.preview_width as f32;
        let img_h = state.preview_height as f32;
        let scale = (preview_max_w / img_w).min(preview_max_h / img_h).min(1.0);
        let display_w = (img_w * scale).max(100.0);
        let display_h = (img_h * scale).max(56.0);

        container(image(handle).width(display_w).height(display_h))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .style(move |_t: &iced_core::Theme| container::Style {
                background: Some(palette.background.weak.color.into()),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: palette.background.strong.color,
                },
                ..Default::default()
            })
            .into()
    } else {
        preview_area_empty(palette)
    }
}

/// 预览区域（无图片时）
pub(super) fn preview_area_empty<'a>(
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    container(
        text("等待渲染...")
            .size(14)
            .style(widgets::dialog_muted_text_style(palette)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(120.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_t: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    })
    .into()
}
