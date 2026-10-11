//! 计数器设置面板 — 字体设置区
//!
//! 字体来源（内置点阵 / 系统字体 / 自定义字体文件）与系统字体选择。
//! 从 `counter_settings.rs` 拆出，控制文件行数。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, pick_list, row, space, text, text_input};
use lumino_core::tr;

use crate::message::{Message, VideoExportAction};
use crate::view::widgets;

use super::localized_choice::LocalizedChoice;
use super::state::VideoExportDialogState;

/// 计数器系统字体可选项（路径解析在 runner 侧，此处仅展示名称）
///
/// ⚠️ 这些是**字体族名**（数据值，用于按名查字体路径），不是界面文案：
/// 绝对不要接入 i18n，也不要翻译。
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
    // 注：下面三个选项值同时是 `state.counter_font_mode` 的持久化取值，
    // 并在 `root/handlers/dialog/video_export` 侧参与 `match` 分派 —— 属数据 key。
    // 状态值与分派键保持中文不动；仅用 `LocalizedChoice` 把「显示」接上文案目录，
    // 选中回调回发原始数据值（`.0`）。
    let font_mode = state.counter_font_mode.as_str();
    let font_mode_pick = pick_list(
        vec![
            LocalizedChoice::new("内置点阵"),
            LocalizedChoice::new("系统字体"),
            LocalizedChoice::new("自定义字体"),
        ],
        Some(LocalizedChoice::new(font_mode)),
        |v: LocalizedChoice| Message::VideoExport(VideoExportAction::CounterFontModeChanged(v.0)),
    )
    .text_size(12)
    .width(Length::Fixed(140.0));

    // 系统字体选择（字体族名是数据值，不是文案，保持原样不动）
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
            text_input(tr!("选择 TTF/OTF 字体文件..."), &state.counter_font_path)
                .on_input(|v| {
                    Message::VideoExport(VideoExportAction::CounterFontPathChanged(v))
                })
                .padding([4, 8])
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .style(widgets::dialog_input_style(palette)),
        space().width(6),
        button(text(tr!("浏览...")).size(12))
            .on_press(Message::VideoExport(VideoExportAction::CounterBrowseFont))
            .padding([4, 10]),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    // 按来源模式显示对应配置行（分支值同上：与 `counter_font_mode` 比对的数据 key）
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
            text(tr!(
                "内置 5x7 点阵字体，仅支持 ASCII；中文模板请选择系统字体或自定义字体"
            ))
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
