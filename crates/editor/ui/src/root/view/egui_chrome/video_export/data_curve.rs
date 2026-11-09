//! 视频导出「数据曲线」设置分区（数据源 / 曲线参数 / 外观 / 文字与刻度）。
//!
//! 对应 iced `view/video_export_dialog/data_curve_settings.rs` +
//! `data_curve_settings_style.rs`。

use lumino_core::tr;
use lumino_ui_egui::chrome::{TextPairCell, VideoButtonFill, VideoRow, VideoSection};

use super::backends::SYSTEM_FONT_FAMILIES;
use crate::state::root_state::VideoExportDialogState;

use super::counter::FONT_MODES;
use super::{button, gap, label, pick, text_button};

/// 数据来源四项（数据 key）。
pub(super) const METRICS: [&str; 4] = ["NPS（每秒音符数）", "复音数", "累计音符数", "BPM（速度）"];

/// 数值输入单元（标签 110 + 输入 72）。
fn number_cell(label: &str, value: &str, id: u16) -> TextPairCell {
    TextPairCell {
        label: label.to_string(),
        label_width: 110.0,
        value: value.to_string(),
        hint: "0".to_string(),
        width: 72.0,
        id,
    }
}

/// 颜色输入单元（标签 110 + 输入 96）。
fn color_cell(label: &str, value: &str, id: u16) -> TextPairCell {
    TextPairCell {
        label: label.to_string(),
        label_width: 110.0,
        value: value.to_string(),
        hint: "#000000".to_string(),
        width: 96.0,
        id,
    }
}

/// 构建数据曲线分区。
pub(super) fn section(state: &VideoExportDialogState) -> VideoSection {
    let mut rows = vec![
        pick(
            tr!("数据来源:"),
            100.0,
            METRICS.iter().map(|s| s.to_string()).collect(),
            METRICS
                .iter()
                .position(|v| *v == state.dc_metric)
                .unwrap_or(0),
            180.0,
            200,
        ),
        gap(4.0),
        label(
            tr!("数据由 MIDI 统计状态按帧直传，无需外部文件"),
            11.0,
            true,
        ),
        gap(12.0),
        label(tr!("曲线参数"), 13.0, false),
        gap(4.0),
        VideoRow::TextPair {
            left: number_cell(tr!("窗口时长(秒)"), &state.dc_graph_duration, 201),
            right: number_cell(tr!("缩放平滑"), &state.dc_zoom_smoothness, 202),
            separator: String::new(),
        },
        gap(6.0),
        VideoRow::TextPair {
            left: number_cell(tr!("折线平滑"), &state.dc_graph_smoothness, 203),
            right: number_cell(tr!("纵轴 padding"), &state.dc_padding_mul, 204),
            separator: String::new(),
        },
        gap(4.0),
        label(tr!("折线平滑=0 关闭；越大曲线越平滑但更趋直线"), 11.0, true),
        gap(12.0),
        label(tr!("外观"), 13.0, false),
        gap(4.0),
        VideoRow::TextPair {
            left: color_cell(tr!("背景色"), &state.dc_bg_color, 210),
            right: color_cell(tr!("折线色"), &state.dc_line_color, 211),
            separator: String::new(),
        },
        gap(6.0),
        VideoRow::TextPair {
            left: color_cell(tr!("文字色"), &state.dc_text_color, 212),
            right: color_cell(tr!("网格线色"), &state.dc_bar_color, 213),
            separator: String::new(),
        },
        gap(6.0),
        VideoRow::TextPair {
            left: number_cell(tr!("折线宽(px)"), &state.dc_line_thickness, 214),
            right: number_cell(tr!("网格线宽(px)"), &state.dc_bar_thickness, 215),
            separator: String::new(),
        },
        gap(6.0),
        VideoRow::Slider {
            label: tr!("字号:").to_string(),
            label_width: 100.0,
            value: state.dc_font_size as f32,
            min: 7.0,
            max: 256.0,
            step: 1.0,
            slider_width: 140.0,
            value_text: format!("{} px", state.dc_font_size),
            id: 216,
        },
        gap(6.0),
        pick(
            tr!("字体:"),
            100.0,
            FONT_MODES.iter().map(|s| s.to_string()).collect(),
            FONT_MODES
                .iter()
                .position(|v| *v == state.dc_font_mode)
                .unwrap_or(0),
            140.0,
            217,
        ),
        gap(6.0),
    ];

    match state.dc_font_mode.as_str() {
        "系统字体" => rows.push(pick(
            tr!("字体:"),
            100.0,
            SYSTEM_FONT_FAMILIES.iter().map(|s| s.to_string()).collect(),
            SYSTEM_FONT_FAMILIES
                .iter()
                .position(|v| *v == state.dc_font_family)
                .unwrap_or(0),
            140.0,
            218,
        )),
        "自定义字体" => rows.push(text_button(
            "",
            0.0,
            219,
            &state.dc_font_path,
            tr!("选择 TTF/OTF 字体文件..."),
            0.0,
            button(tr!("浏览..."), 220, VideoButtonFill::Primary, 0.0),
        )),
        _ => rows.push(label(
            tr!("内置 5x7 点阵字体，仅支持 ASCII 数字与字母"),
            11.0,
            true,
        )),
    }

    rows.push(gap(4.0));
    rows.push(label(
        tr!("支持 8 位 hex 带透明度，如 #FFFFFF7F"),
        11.0,
        true,
    ));
    rows.push(gap(12.0));
    rows.push(label(tr!("文字与刻度"), 13.0, false));
    rows.push(gap(4.0));
    rows.push(VideoRow::TextPair {
        left: number_cell(tr!("X 偏移(px)"), &state.dc_text_x_offset, 230),
        right: number_cell(tr!("Y 偏移(px)"), &state.dc_text_y_offset, 231),
        separator: String::new(),
    });
    rows.push(gap(6.0));
    rows.push(VideoRow::TextPair {
        left: number_cell(tr!("里程碑放大"), &state.dc_milestone_scale_mul, 232),
        right: number_cell(tr!("缩写位数"), &state.dc_abbreviate_digits, 233),
        separator: String::new(),
    });
    rows.push(gap(6.0));
    rows.push(VideoRow::Toggle {
        label: tr!("刻度数字缩写（1,000 → 1K）").to_string(),
        checked: state.dc_abbreviate,
        id: 234,
    });
    rows.push(gap(4.0));
    rows.push(VideoRow::Toggle {
        label: tr!("显示刻度文字").to_string(),
        checked: state.dc_show_text,
        id: 235,
    });
    rows.push(gap(4.0));
    rows.push(VideoRow::Toggle {
        label: tr!("显示水平网格线").to_string(),
        checked: state.dc_show_bars,
        id: 236,
    });
    rows.push(gap(4.0));
    rows.push(label(
        tr!("里程碑 = 1,000 / 10,000 / 100,000 等整数次幂刻度，文字放大显示"),
        11.0,
        true,
    ));

    VideoSection {
        title: tr!("数据曲线设置").to_string(),
        rows,
    }
}
