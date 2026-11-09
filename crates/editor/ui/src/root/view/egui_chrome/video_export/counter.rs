//! 视频导出「计数器」设置分区（模板 / 字体 / 外观 / CSV / 补零宽度）。
//!
//! 对应 iced `view/video_export_dialog/counter_settings.rs` +
//! `counter_settings_font.rs`；控件全部由 `VideoRow` 描述，生产侧只提供数据。

use lumino_core::tr;
use lumino_ui_egui::chrome::{TextPairCell, VideoButton, VideoButtonFill, VideoRow, VideoSection};

use super::backends::SYSTEM_FONT_FAMILIES;
use crate::state::root_state::VideoExportDialogState;

use super::{button, gap, label, pick, text_button};

/// 计数器字体来源三项（数据 key，不翻译）。
pub(super) const FONT_MODES: [&str; 3] = ["内置点阵", "系统字体", "自定义字体"];
/// 计数器六种对齐（数据 key）。
pub(super) const ALIGNMENTS: [&str; 6] = ["左上", "右上", "左下", "右下", "顶部分散", "底部分散"];
/// 计数器八个补零字段（`CounterPadChanged.field`）。
pub(super) const PAD_FIELDS: [&str; 8] = [
    "bpm_int", "bpm_dec", "nc", "plph", "nps", "ticks", "bars", "frames",
];

/// 紧凑数值输入单元（标签 70 + 输入 56）。
fn pad_cell(label: &str, value: u32, id: u16) -> TextPairCell {
    TextPairCell {
        label: label.to_string(),
        label_width: 70.0,
        value: value.to_string(),
        hint: "0".to_string(),
        width: 56.0,
        id,
    }
}

/// 构建计数器分区。
pub(super) fn section(state: &VideoExportDialogState) -> VideoSection {
    let mut rows = vec![
        label(
            tr!("文本模板（支持 {nc} {nps} {bpm} 等占位符，\\n 换行）"),
            12.0,
            true,
        ),
        gap(4.0),
        VideoRow::TextArea {
            label: String::new(),
            value: state.counter_text.clone(),
            hint: "Notes: {nc} / {tn}\\nBPM: {bpm}".to_string(),
            height: 140.0,
            id: 100,
        },
        gap(4.0),
        VideoRow::Buttons {
            buttons: vec![
                button(tr!("恢复默认"), 101, VideoButtonFill::Primary, 0.0),
                button(tr!("完整模板"), 102, VideoButtonFill::Primary, 0.0),
            ],
        },
        gap(12.0),
        pick(
            tr!("字体:"),
            100.0,
            FONT_MODES.iter().map(|s| s.to_string()).collect(),
            FONT_MODES
                .iter()
                .position(|v| *v == state.counter_font_mode)
                .unwrap_or(0),
            140.0,
            103,
        ),
        gap(6.0),
    ];

    // 字体来源分支（与 iced 同：系统字体 / 自定义字体 / 内置点阵提示）。
    match state.counter_font_mode.as_str() {
        "系统字体" => rows.push(pick(
            tr!("字体:"),
            100.0,
            SYSTEM_FONT_FAMILIES.iter().map(|s| s.to_string()).collect(),
            SYSTEM_FONT_FAMILIES
                .iter()
                .position(|v| *v == state.counter_font_family)
                .unwrap_or(0),
            140.0,
            104,
        )),
        "自定义字体" => rows.push(text_button(
            "",
            0.0,
            105,
            &state.counter_font_path,
            tr!("选择 TTF/OTF 字体文件..."),
            0.0,
            button(tr!("浏览..."), 106, VideoButtonFill::Primary, 0.0),
        )),
        _ => rows.push(label(
            tr!("内置 5x7 点阵字体，仅支持 ASCII；中文模板请选择系统字体或自定义字体"),
            11.0,
            true,
        )),
    }

    rows.push(gap(8.0));
    rows.push(pick(
        tr!("对齐:"),
        100.0,
        ALIGNMENTS.iter().map(|s| s.to_string()).collect(),
        ALIGNMENTS
            .iter()
            .position(|v| *v == state.counter_alignment)
            .unwrap_or(0),
        140.0,
        107,
    ));
    rows.push(gap(8.0));
    rows.push(VideoRow::Slider {
        label: tr!("字号:").to_string(),
        label_width: 100.0,
        value: state.counter_font_size as f32,
        min: 7.0,
        max: 256.0,
        step: 1.0,
        slider_width: 140.0,
        value_text: format!("{} px", state.counter_font_size),
        id: 108,
    });
    rows.push(gap(8.0));
    rows.push(VideoRow::Toggle {
        label: tr!("使用千分位分隔符（1,234,567）").to_string(),
        checked: state.counter_use_commas,
        id: 109,
    });
    rows.push(gap(4.0));
    rows.push(VideoRow::Toggle {
        label: tr!("数字补零（按下方宽度对齐）").to_string(),
        checked: state.counter_padding_zeroes,
        id: 110,
    });
    rows.push(gap(12.0));

    // CSV 导出。
    rows.push(VideoRow::Toggle {
        label: tr!("每帧统计数据写入 CSV 文件").to_string(),
        checked: state.counter_save_csv,
        id: 111,
    });
    if state.counter_save_csv {
        rows.push(gap(4.0));
        rows.push(text_button(
            tr!("路径:"),
            50.0,
            112,
            &state.counter_csv_output,
            tr!("选择 CSV 输出路径..."),
            0.0,
            button(tr!("浏览..."), 113, VideoButtonFill::Primary, 0.0),
        ));
        rows.push(gap(4.0));
        rows.push(VideoRow::Text {
            label: tr!("格式:").to_string(),
            label_width: 50.0,
            value: state.counter_csv_format.clone(),
            hint: "{nps},{plph},{bpm},{nc}".to_string(),
            width: 0.0,
            id: 114,
        });
    }
    rows.push(gap(12.0));

    // 数字补零宽度（8 项，两列四行）+ 恢复默认。
    rows.push(label(tr!("数字补零宽度"), 13.0, false));
    rows.push(gap(4.0));
    let pads: [(TextPairCell, TextPairCell); 4] = [
        (
            pad_cell(tr!("BPM 整数"), state.counter_bpm_int_pad, 120),
            pad_cell(tr!("BPM 小数"), state.counter_bpm_dec_pad, 121),
        ),
        (
            pad_cell(tr!("音符数"), state.counter_note_count_pad, 122),
            pad_cell(tr!("复音数"), state.counter_polyphony_pad, 123),
        ),
        (
            pad_cell("NPS", state.counter_nps_pad, 124),
            pad_cell("Ticks", state.counter_ticks_pad, 125),
        ),
        (
            pad_cell(tr!("小节"), state.counter_bars_pad, 126),
            pad_cell(tr!("帧数"), state.counter_frames_pad, 127),
        ),
    ];
    for (index, (left, right)) in pads.into_iter().enumerate() {
        if index > 0 {
            rows.push(gap(6.0));
        }
        rows.push(VideoRow::TextPair {
            left,
            right,
            separator: String::new(),
        });
    }
    rows.push(gap(8.0));
    rows.push(VideoRow::Buttons {
        buttons: vec![VideoButton {
            label: tr!("恢复默认补零宽度").to_string(),
            id: 128,
            fill: VideoButtonFill::Primary,
            width: 0.0,
        }],
    });

    VideoSection {
        title: tr!("计数器设置").to_string(),
        rows,
    }
}
