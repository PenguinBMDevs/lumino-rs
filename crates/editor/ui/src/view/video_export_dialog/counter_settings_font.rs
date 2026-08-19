//! 计数器设置面板 — 字体设置区
//!
//! 字体来源（内置点阵 / 系统字体 / 自定义字体文件）与系统字体选择。
//! 从 `counter_settings.rs` 拆出，控制文件行数。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, pick_list, row, space, text, text_input};

use crate::message::{Message, VideoExportAction};
use crate::view::widgets;

use super::state::VideoExportDialogState;

/// 计数器系统字体可选项（路径解析在 runner 侧，此处仅展示名称）
pub const SYSTEM_FONT_FAMILIES: [&str; 8] = [
    "微软雅黑",
    "微软雅黑粗体",
    "宋体",
    "黑体",
    "楷体",
    "仿宋",
    "Arial",
    "Consolas",
];

/// 字体设置区（内置点阵 / 系统字体 / 自定义字体文件）。
pub fn font_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    // 字体来源选择
    let font_mode = state.counter_font_mode.as_str();
    let font_mode_pick = pick_list(
        vec![
            "内置点阵".to_string(),
            "系统字体".to_string(),
            "自定义字体".to_string(),
        ],
        Some(font_mode.to_string()),
        |v| Message::VideoExport(VideoExportAction::CounterFontModeChanged(v)),
    )
    .text_size(12)
    .width(Length::Fixed(140.0));

    // 系统字体选择
    let system_pick = pick_list(
        SYSTEM_FONT_FAMILIES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        Some(state.counter_font_family.clone()),
        |v| Message::VideoExport(VideoExportAction::CounterFontFamilyChanged(v)),
    )
    .text_size(12)
    .width(Length::Fixed(140.0));

    // 自定义字体文件路径（含浏览按钮）
    let file_row = row![
        container(
            text_input("选择 TTF/OTF 字体文件...", &state.counter_font_path)
                .on_input(|v| {
                    Message::VideoExport(VideoExportAction::CounterFontPathChanged(v))
                })
                .padding([4, 8])
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .style(widgets::dialog_input_style(palette)),
        space().width(6),
        button(text("浏览...").size(12))
            .on_press(Message::VideoExport(VideoExportAction::CounterBrowseFont))
            .padding([4, 10]),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    // 按来源模式显示对应配置行
    let font_extras: crate::Element<'a> = match font_mode {
        "系统字体" => row![
            text("字体:").size(14).style(label_style).width(100),
            system_pick,
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        "自定义字体" => file_row.into(),
        _ => column![
            text("内置 5x7 点阵字体，仅支持 ASCII；中文模板请选择系统字体或自定义字体")
                .size(11)
                .style(label_style),
        ]
        .width(Length::Fill)
        .into(),
    };

    column![
        row![
            text("字体:").size(14).style(label_style).width(100),
            font_mode_pick,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(6),
        font_extras,
    ]
    .width(Length::Fill)
    .into()
}
