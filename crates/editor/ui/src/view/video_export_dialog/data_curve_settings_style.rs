//! 视频导出对话框 — 数据曲线外观设置区（颜色 / 线宽 / 字号 / 字体）
//!
//! 从 data_curve_settings.rs 拆出，控制单文件行数（<400 行）。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, pick_list, row, slider, space, text, text_input};

use lumino_core::tr;

use crate::message::{Message, VideoExportAction};
use crate::view::widgets;

use super::data_curve_settings::number_row;
use super::localized_choice::LocalizedChoice;
use super::state::VideoExportDialogState;

/// hex 颜色输入行
fn color_row<'a>(
    label: &'a str,
    value: &'a str,
    field: &'a str,
    palette: &'a iced_core::theme::palette::Extended,
    label_style: impl Fn(&iced_core::Theme) -> iced_widget::text::Style + 'static,
) -> crate::Element<'a> {
    row![
        text(label).size(13).style(label_style).width(110),
        container(
            text_input("#000000", value)
                .on_input(move |v| {
                    Message::VideoExport(VideoExportAction::DataCurveTextChanged {
                        field: field.to_string(),
                        value: v,
                    })
                })
                .padding([3, 6])
                .width(Length::Fixed(96.0)),
        )
        .style(widgets::dialog_input_style(palette)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// 外观区：四色、线宽、网格线宽、字号、字体
pub(super) fn appearance_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    column![
        text(tr!("外观")).size(13).style(label_style),
        space().height(4),
        row![
            color_row(
                tr!("背景色"),
                &state.dc_bg_color,
                "bg_color",
                palette,
                label_style
            ),
            space().width(8),
            color_row(
                tr!("折线色"),
                &state.dc_line_color,
                "line_color",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            color_row(
                tr!("文字色"),
                &state.dc_text_color,
                "text_color",
                palette,
                label_style
            ),
            space().width(8),
            color_row(
                tr!("网格线色"),
                &state.dc_bar_color,
                "bar_color",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            number_row(
                tr!("折线宽(px)"),
                &state.dc_line_thickness,
                "line_thickness",
                palette,
                label_style
            ),
            space().width(8),
            number_row(
                tr!("网格线宽(px)"),
                &state.dc_bar_thickness,
                "bar_thickness",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            text(tr!("字号:")).size(14).style(label_style).width(100),
            slider(7..=256, state.dc_font_size, |v| {
                Message::VideoExport(VideoExportAction::DataCurveFontSizeChanged(v))
            })
            .step(1u32)
            .width(140.0),
            text(format!("{} px", state.dc_font_size))
                .size(12)
                .style(label_style),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(6),
        font_row(state, palette),
        space().height(4),
        text(tr!("支持 8 位 hex 带透明度，如 #FFFFFF7F"))
            .size(11)
            .style(label_style),
    ]
    .width(Length::Fill)
    .into()
}

/// 字体来源行（内置点阵 / 系统字体 / 自定义字体文件）
fn font_row<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    let font_mode = state.dc_font_mode.as_str();
    // 注：三个选项值同时是 `state.dc_font_mode` 的持久化取值，并在
    // `root/handlers/dialog/video_export` 侧参与 `match` 分派 —— 属数据 key，
    // 状态里保持中文不动；`LocalizedChoice` 只本地化显示，回发原始值（`.0`）。
    let font_mode_pick = pick_list(
        vec![
            LocalizedChoice::new("内置点阵"),
            LocalizedChoice::new("系统字体"),
            LocalizedChoice::new("自定义字体"),
        ],
        Some(LocalizedChoice::new(font_mode)),
        |v: LocalizedChoice| Message::VideoExport(VideoExportAction::DataCurveFontModeChanged(v.0)),
    )
    .text_size(12)
    .width(Length::Fixed(140.0));

    // 系统字体选择（字体族名是数据值，不是文案，保持原样不动）
    let system_pick = pick_list(
        super::counter_settings_font::SYSTEM_FONT_FAMILIES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        Some(state.dc_font_family.clone()),
        |v| {
            Message::VideoExport(VideoExportAction::DataCurveTextChanged {
                field: "font_family".to_string(),
                value: v,
            })
        },
    )
    .text_size(12)
    .width(Length::Fixed(140.0));

    let file_row = row![
        container(
            text_input(tr!("选择 TTF/OTF 字体文件..."), &state.dc_font_path)
                .on_input(|v| {
                    Message::VideoExport(VideoExportAction::DataCurveTextChanged {
                        field: "font_path".to_string(),
                        value: v,
                    })
                })
                .padding([4, 8])
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .style(widgets::dialog_input_style(palette)),
        space().width(6),
        button(text(tr!("浏览...")).size(12))
            .on_press(Message::VideoExport(VideoExportAction::DataCurveBrowseFont))
            .padding([4, 10]),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    // 分支值同上是数据 key（与 `dc_font_mode` 比对），状态与分派键不翻译，
    // 只有上面的 `font_mode_pick` 显示层接了文案目录。
    let font_extras: crate::Element<'a> = match font_mode {
        "系统字体" => row![
            text(tr!("字体:")).size(14).style(label_style).width(100),
            system_pick,
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        "自定义字体" => file_row.into(),
        _ => column![
            text(tr!("内置 5x7 点阵字体，仅支持 ASCII 数字与字母"))
                .size(11)
                .style(label_style),
        ]
        .width(Length::Fill)
        .into(),
    };

    column![
        row![
            text(tr!("字体:")).size(14).style(label_style).width(100),
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
