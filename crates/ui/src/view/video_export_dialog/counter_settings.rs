//! 视频导出对话框 — 计数器设置区块
//!
//! 参考 Zenith-MIDI NoteCountRender 的 SettingsCtrl（Render / SaveCSV / Padding 三个
//! 设置区）：
//! - 文本模板：多行编辑器 + 恢复默认 / 完整模板
//! - 外观：对齐方式（六种）、字号、千分位、补零
//! - CSV：导出开关、路径、行格式
//! - 数字补零宽度：8 项 + 恢复默认
//!
//! 仅当渲染风格为「计数器」时显示（由调用方控制）。

use iced_core::{Alignment, Length};
use iced_widget::{
    button, checkbox, column, container, pick_list, row, slider, space, text, text_input,
};

use crate::message::{Message, VideoExportAction};
use crate::view::widgets;

use super::state::VideoExportDialogState;

/// 计数器设置区块（仅「计数器」渲染风格时显示）
pub fn counter_settings_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("计数器设置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        // ── 文本模板（Zenith SettingsCtrl 的 Render Tab 主体） ──
        template_section(state, palette),
        space().height(12),
        // ── 外观 ──
        appearance_section(state, palette),
        space().height(12),
        // ── CSV 导出（Zenith SaveCSV Tab） ──
        csv_section(state, palette),
        space().height(12),
        // ── 数字补零宽度（Zenith Padding Tab） ──
        padding_section(state, palette),
    ]
    .width(Length::Fill)
    .into()
}

/// 文本模板区：多行编辑器 + 预设按钮
fn template_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    let editor = iced_widget::text_editor::TextEditor::new(&state.counter_editor)
        .on_action(|action| Message::VideoExport(VideoExportAction::CounterTextAction(action)))
        .placeholder("Notes: {nc} / {tn}\nBPM: {bpm}")
        .height(iced_core::Pixels(140.0))
        .padding(8);

    column![
        text("文本模板（支持 {nc} {nps} {bpm} 等占位符，\\n 换行）")
            .size(12)
            .style(label_style),
        space().height(4),
        container(editor)
            .width(Length::Fill)
            .style(widgets::dialog_input_style(palette)),
        space().height(4),
        row![
            button(text("恢复默认").size(12))
                .on_press(Message::VideoExport(
                    VideoExportAction::CounterLoadTemplate("default".to_string(),)
                ))
                .padding([4, 12]),
            space().width(8),
            button(text("完整模板").size(12))
                .on_press(Message::VideoExport(
                    VideoExportAction::CounterLoadTemplate("full".to_string(),)
                ))
                .padding([4, 12]),
        ]
        .spacing(4),
    ]
    .width(Length::Fill)
    .into()
}

/// 外观区：对齐、字号、千分位、补零
fn appearance_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    column![
        row![
            text("对齐:").size(14).style(label_style).width(100),
            pick_list(
                vec![
                    "左上".to_string(),
                    "右上".to_string(),
                    "左下".to_string(),
                    "右下".to_string(),
                    "顶部分散".to_string(),
                    "底部分散".to_string(),
                ],
                Some(state.counter_alignment.clone()),
                |v| Message::VideoExport(VideoExportAction::CounterAlignmentChanged(v)),
            )
            .width(Length::Fixed(140.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        row![
            text("字号:").size(14).style(label_style).width(100),
            slider(7..=256, state.counter_font_size, |v| {
                Message::VideoExport(VideoExportAction::CounterFontSizeChanged(v))
            })
            .step(1u32)
            .width(140.0),
            text(format!("{} px", state.counter_font_size))
                .size(12)
                .style(label_style),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        checkbox(state.counter_use_commas)
            .label("使用千分位分隔符（1,234,567）")
            .on_toggle(|v| Message::VideoExport(VideoExportAction::CounterUseCommasChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        checkbox(state.counter_padding_zeroes)
            .label("数字补零（按下方宽度对齐）")
            .on_toggle(|v| {
                Message::VideoExport(VideoExportAction::CounterPaddingZeroesChanged(v))
            })
            .style(widgets::dialog_checkbox_style(palette)),
    ]
    .width(Length::Fill)
    .into()
}

/// CSV 导出区
fn csv_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| iced_widget::text::Style {
        color: Some(label_color),
    };

    let csv_extras: crate::Element<'a> = if state.counter_save_csv {
        column![
            space().height(4),
            row![
                text("路径:").size(12).style(label_style).width(50),
                container(
                    text_input("选择 CSV 输出路径...", &state.counter_csv_output)
                        .on_input(|v| {
                            Message::VideoExport(VideoExportAction::CounterCsvPathChanged(v))
                        })
                        .padding([4, 8])
                        .width(Length::Fill),
                )
                .width(Length::Fill)
                .style(widgets::dialog_input_style(palette)),
                space().width(6),
                button(text("浏览...").size(12))
                    .on_press(Message::VideoExport(VideoExportAction::CounterBrowseCsv))
                    .padding([4, 10]),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
            space().height(4),
            row![
                text("格式:").size(12).style(label_style).width(50),
                container(
                    text_input("{nps},{plph},{bpm},{nc}", &state.counter_csv_format)
                        .on_input(|v| {
                            Message::VideoExport(VideoExportAction::CounterCsvFormatChanged(v))
                        })
                        .padding([4, 8])
                        .width(Length::Fill),
                )
                .width(Length::Fill)
                .style(widgets::dialog_input_style(palette)),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        ]
        .width(Length::Fill)
        .into()
    } else {
        space().height(0).into()
    };

    column![
        checkbox(state.counter_save_csv)
            .label("每帧统计数据写入 CSV 文件")
            .on_toggle(|v| Message::VideoExport(VideoExportAction::CounterSaveCsvChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        csv_extras,
    ]
    .width(Length::Fill)
    .into()
}

/// 数字补零宽度区（8 项 + 恢复默认）
fn padding_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    // 紧凑输入行：标签 + 数字输入
    fn pad_row<'a>(
        label: &'a str,
        value: u32,
        field: &'a str,
        palette: &'a iced_core::theme::palette::Extended,
        label_style: impl Fn(&iced_core::Theme) -> iced_widget::text::Style + 'static,
    ) -> crate::Element<'a> {
        row![
            text(label).size(12).style(label_style).width(70),
            container(
                text_input("0", &value.to_string())
                    .on_input(move |v| {
                        let parsed = v.trim().parse::<u32>().unwrap_or(0);
                        Message::VideoExport(VideoExportAction::CounterPadChanged {
                            field: field.to_string(),
                            value: parsed,
                        })
                    })
                    .padding([3, 6])
                    .width(Length::Fixed(56.0)),
            )
            .style(widgets::dialog_input_style(palette)),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    column![
        text("数字补零宽度").size(13).style(label_style),
        space().height(4),
        row![
            pad_row(
                "BPM 整数",
                state.counter_bpm_int_pad,
                "bpm_int",
                palette,
                label_style
            ),
            space().width(8),
            pad_row(
                "BPM 小数",
                state.counter_bpm_dec_pad,
                "bpm_dec",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            pad_row(
                "音符数",
                state.counter_note_count_pad,
                "nc",
                palette,
                label_style
            ),
            space().width(8),
            pad_row(
                "复音数",
                state.counter_polyphony_pad,
                "plph",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            pad_row("NPS", state.counter_nps_pad, "nps", palette, label_style),
            space().width(8),
            pad_row(
                "Ticks",
                state.counter_ticks_pad,
                "ticks",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            pad_row("小节", state.counter_bars_pad, "bars", palette, label_style),
            space().width(8),
            pad_row(
                "帧数",
                state.counter_frames_pad,
                "frames",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(8),
        button(text("恢复默认补零宽度").size(12))
            .on_press(Message::VideoExport(VideoExportAction::CounterResetPadding))
            .padding([4, 12]),
    ]
    .width(Length::Fill)
    .into()
}
