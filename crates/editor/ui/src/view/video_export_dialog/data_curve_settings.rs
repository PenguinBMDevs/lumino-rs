//! 视频导出对话框 — 数据曲线设置区块
//!
//! 移植自 MIDIGraphRenderer（LÖVE2D）的 graph 设置面板：
//! - 数据源：内部统计指标直传（NPS / 复音数 / 累计音符数 / BPM）
//! - 曲线：窗口时长、缩放平滑、折线平滑、纵轴 padding
//! - 外观：背景 / 折线 / 文字 / 网格线四色、线宽、字号、字体
//! - 文字：偏移、里程碑放大、数字缩写、显示开关
//!
//! 仅当渲染风格为「数据曲线」时显示（由调用方控制）。

use iced_core::{Alignment, Length};
use iced_widget::{checkbox, column, container, pick_list, row, space, text, text_input};

use crate::message::{Message, VideoExportAction};
use crate::view::widgets;

use super::state::VideoExportDialogState;

/// 数据曲线设置区块（仅「数据曲线」渲染风格时显示）
pub fn data_curve_settings_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("数据曲线设置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        // ── 数据源 ──
        metric_section(state, palette),
        space().height(12),
        // ── 曲线参数 ──
        curve_section(state, palette),
        space().height(12),
        // ── 外观 ──
        super::data_curve_settings_style::appearance_section(state, palette),
        space().height(12),
        // ── 文字与刻度 ──
        text_section(state, palette),
    ]
    .width(Length::Fill)
    .into()
}

/// 数字输入行（label + text_input，值回写指定 field）
pub(super) fn number_row<'a>(
    label: &'a str,
    value: &'a str,
    field: &'a str,
    palette: &'a iced_core::theme::palette::Extended,
    label_style: impl Fn(&iced_core::Theme) -> iced_widget::text::Style + 'static,
) -> crate::Element<'a> {
    row![
        text(label).size(13).style(label_style).width(110),
        container(
            text_input("0", value)
                .on_input(move |v| {
                    let parsed = v.trim().parse::<f32>().unwrap_or(0.0);
                    Message::VideoExport(VideoExportAction::DataCurveNumberChanged {
                        field: field.to_string(),
                        value: parsed,
                    })
                })
                .padding([3, 6])
                .width(Length::Fixed(72.0)),
        )
        .style(widgets::dialog_input_style(palette)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// 数据源区：指标选择
fn metric_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    column![
        row![
            text("数据来源:").size(14).style(label_style).width(100),
            pick_list(
                vec![
                    "NPS（每秒音符数）".to_string(),
                    "复音数".to_string(),
                    "累计音符数".to_string(),
                    "BPM（速度）".to_string(),
                ],
                Some(state.dc_metric.clone()),
                |v| Message::VideoExport(VideoExportAction::DataCurveTextChanged {
                    field: "metric".to_string(),
                    value: v,
                }),
            )
            .text_size(12)
            .width(Length::Fixed(180.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(4),
        text("数据由 MIDI 统计状态按帧直传，无需外部文件")
            .size(11)
            .style(label_style),
    ]
    .width(Length::Fill)
    .into()
}

/// 曲线参数区：窗口时长 / 缩放平滑 / 折线平滑 / 纵轴 padding
fn curve_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    column![
        text("曲线参数").size(13).style(label_style),
        space().height(4),
        row![
            number_row(
                "窗口时长(秒)",
                &state.dc_graph_duration,
                "graph_duration",
                palette,
                label_style
            ),
            space().width(8),
            number_row(
                "缩放平滑",
                &state.dc_zoom_smoothness,
                "zoom_smoothness",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            number_row(
                "折线平滑",
                &state.dc_graph_smoothness,
                "graph_smoothness",
                palette,
                label_style
            ),
            space().width(8),
            number_row(
                "纵轴 padding",
                &state.dc_padding_mul,
                "padding_mul",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(4),
        text("折线平滑=0 关闭；越大曲线越平滑但更趋直线")
            .size(11)
            .style(label_style),
    ]
    .width(Length::Fill)
    .into()
}

/// 文字与刻度区：偏移、里程碑放大、缩写、显示开关
fn text_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };

    column![
        text("文字与刻度").size(13).style(label_style),
        space().height(4),
        row![
            number_row(
                "X 偏移(px)",
                &state.dc_text_x_offset,
                "text_x_offset",
                palette,
                label_style
            ),
            space().width(8),
            number_row(
                "Y 偏移(px)",
                &state.dc_text_y_offset,
                "text_y_offset",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        row![
            number_row(
                "里程碑放大",
                &state.dc_milestone_scale_mul,
                "milestone_scale_mul",
                palette,
                label_style
            ),
            space().width(8),
            number_row(
                "缩写位数",
                &state.dc_abbreviate_digits,
                "abbreviate_digits",
                palette,
                label_style
            ),
        ]
        .spacing(0),
        space().height(6),
        checkbox(state.dc_abbreviate)
            .label("刻度数字缩写（1,000 → 1K）")
            .on_toggle(
                |v| Message::VideoExport(VideoExportAction::DataCurveBoolChanged {
                    field: "abbreviate".to_string(),
                    value: v,
                })
            )
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        checkbox(state.dc_show_text)
            .label("显示刻度文字")
            .on_toggle(
                |v| Message::VideoExport(VideoExportAction::DataCurveBoolChanged {
                    field: "show_text".to_string(),
                    value: v,
                })
            )
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        checkbox(state.dc_show_bars)
            .label("显示水平网格线")
            .on_toggle(
                |v| Message::VideoExport(VideoExportAction::DataCurveBoolChanged {
                    field: "show_bars".to_string(),
                    value: v,
                })
            )
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        text("里程碑 = 1,000 / 10,000 / 100,000 等整数次幂刻度，文字放大显示")
            .size(11)
            .style(label_style),
    ]
    .width(Length::Fill)
    .into()
}
