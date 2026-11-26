//! 视频导出面板的行构造小工具（供主模型与计数器 / 数据曲线分区共用）。

use lumino_ui_egui::chrome::{VideoButton, VideoButtonFill, VideoRow};

pub(super) fn pick(
    label: &str,
    label_width: f32,
    options: Vec<String>,
    selected: usize,
    width: f32,
    id: u16,
) -> VideoRow {
    VideoRow::Pick {
        label: label.to_string(),
        label_width,
        options,
        selected,
        width,
        id,
    }
}

pub(super) fn gap(height: f32) -> VideoRow {
    VideoRow::Gap { height }
}

pub(super) fn text_button(
    label: &str,
    label_width: f32,
    text_id: u16,
    value: &str,
    hint: &str,
    width: f32,
    button: VideoButton,
) -> VideoRow {
    VideoRow::TextButton {
        label: label.to_string(),
        label_width,
        value: value.to_string(),
        hint: hint.to_string(),
        width,
        text_id,
        button,
    }
}

pub(super) fn text(
    label: &str,
    label_width: f32,
    value: &str,
    hint: &str,
    width: f32,
    id: u16,
) -> VideoRow {
    VideoRow::Text {
        label: label.to_string(),
        label_width,
        value: value.to_string(),
        hint: hint.to_string(),
        width,
        id,
    }
}

pub(super) fn label(text: &str, size: f32, muted: bool) -> VideoRow {
    VideoRow::Label {
        text: text.to_string(),
        size,
        muted,
    }
}

pub(super) fn button(label: &str, id: u16, fill: VideoButtonFill, width: f32) -> VideoButton {
    VideoButton {
        label: label.to_string(),
        id,
        fill,
        width,
    }
}

/// 行标识（`VideoRow` / `EguiAction` 的 `u16` id）。
///
/// 之前这些数字散落在 41 个 match 臂与构造点里，改名/删除无法被编译器发现。
/// 先集中成常量，下一步再换成 newtype 枚举（T-21e）。
pub(super) mod ids {
    // Pick
    pub const CONTAINER: u16 = 1;
    pub const CODEC: u16 = 2;
    pub const BACKEND: u16 = 3;
    pub const QUALITY: u16 = 4;
    pub const RENDER_MODE: u16 = 5;
    pub const MIDITRAIL_VIEW_MODE: u16 = 7;
    pub const FPS: u16 = 12;
    pub const MIDI_CONSOLE_BACKEND: u16 = 13;
    pub const COUNTER_FONT_MODE: u16 = 103;
    pub const COUNTER_FONT_FAMILY: u16 = 104;
    pub const COUNTER_ALIGNMENT: u16 = 107;
    pub const DC_METRIC: u16 = 200;
    pub const DC_FONT_MODE: u16 = 217;
    pub const DC_FONT_FAMILY: u16 = 218;

    // Slider
    pub const WATERFALL_SPEED: u16 = 6;
    pub const MIDITRAIL_Z_FAR: u16 = 8;
    pub const COUNTER_FONT_SIZE: u16 = 108;
    pub const DC_FONT_SIZE: u16 = 216;

    // Toggle
    pub const MIDITRAIL_3D_NOTES: u16 = 9;
    pub const COUNTER_USE_COMMAS: u16 = 109;
    pub const COUNTER_PADDING_ZEROES: u16 = 110;
    pub const COUNTER_SAVE_CSV: u16 = 111;
    pub const DC_ABBREVIATE: u16 = 234;
    pub const DC_SHOW_TEXT: u16 = 235;
    pub const DC_SHOW_BARS: u16 = 236;

    // Text
    pub const WIDTH: u16 = 10;
    pub const HEIGHT: u16 = 11;
    pub const OUTPUT_PATH: u16 = 14;
    pub const COUNTER_TEXT: u16 = 100;
    pub const COUNTER_FONT_PATH: u16 = 105;
    pub const COUNTER_CSV_PATH: u16 = 112;
    pub const COUNTER_CSV_FORMAT: u16 = 114;
    pub const COUNTER_PAD_FIRST: u16 = 120;
    pub const COUNTER_PAD_LAST: u16 = 127;
    pub const DC_GRAPH_DURATION: u16 = 201;
    pub const DC_ZOOM_SMOOTHNESS: u16 = 202;
    pub const DC_GRAPH_SMOOTHNESS: u16 = 203;
    pub const DC_PADDING_MUL: u16 = 204;
    pub const DC_BG_COLOR: u16 = 210;
    pub const DC_LINE_COLOR: u16 = 211;
    pub const DC_TEXT_COLOR: u16 = 212;
    pub const DC_BAR_COLOR: u16 = 213;
    pub const DC_LINE_THICKNESS: u16 = 214;
    pub const DC_BAR_THICKNESS: u16 = 215;
    pub const DC_FONT_PATH: u16 = 219;
    pub const DC_TEXT_X_OFFSET: u16 = 230;
    pub const DC_TEXT_Y_OFFSET: u16 = 231;
    pub const DC_MILESTONE_SCALE: u16 = 232;
    pub const DC_ABBREVIATE_DIGITS: u16 = 233;

    // Button
    pub const BROWSE_MIDI: u16 = 15;
    pub const BROWSE_OUTPUT: u16 = 16;
    pub const CLOSE_PANEL: u16 = 17;
    pub const START_EXPORT: u16 = 18;
    pub const COUNTER_TEMPLATE_DEFAULT: u16 = 101;
    pub const COUNTER_TEMPLATE_FULL: u16 = 102;
    pub const COUNTER_BROWSE_FONT: u16 = 106;
    pub const COUNTER_BROWSE_CSV: u16 = 113;
    pub const COUNTER_RESET_PADDING: u16 = 128;
    pub const DC_BROWSE_FONT: u16 = 220;
}

#[cfg(test)]
mod tests {
    use super::ids::*;

    /// 行 id 必须唯一：复制粘贴新增行时最容易撞号，撞号会让 dispatcher 静默走错分支。
    #[test]
    fn row_ids_are_unique() {
        let all = [
            CONTAINER,
            CODEC,
            BACKEND,
            QUALITY,
            RENDER_MODE,
            MIDITRAIL_VIEW_MODE,
            FPS,
            MIDI_CONSOLE_BACKEND,
            COUNTER_FONT_MODE,
            COUNTER_FONT_FAMILY,
            COUNTER_ALIGNMENT,
            DC_METRIC,
            DC_FONT_MODE,
            DC_FONT_FAMILY,
            WATERFALL_SPEED,
            MIDITRAIL_Z_FAR,
            COUNTER_FONT_SIZE,
            DC_FONT_SIZE,
            MIDITRAIL_3D_NOTES,
            COUNTER_USE_COMMAS,
            COUNTER_PADDING_ZEROES,
            COUNTER_SAVE_CSV,
            DC_ABBREVIATE,
            DC_SHOW_TEXT,
            DC_SHOW_BARS,
            WIDTH,
            HEIGHT,
            OUTPUT_PATH,
            COUNTER_TEXT,
            COUNTER_FONT_PATH,
            COUNTER_CSV_PATH,
            COUNTER_CSV_FORMAT,
            DC_GRAPH_DURATION,
            DC_ZOOM_SMOOTHNESS,
            DC_GRAPH_SMOOTHNESS,
            DC_PADDING_MUL,
            DC_BG_COLOR,
            DC_LINE_COLOR,
            DC_TEXT_COLOR,
            DC_BAR_COLOR,
            DC_LINE_THICKNESS,
            DC_BAR_THICKNESS,
            DC_FONT_PATH,
            DC_TEXT_X_OFFSET,
            DC_TEXT_Y_OFFSET,
            DC_MILESTONE_SCALE,
            DC_ABBREVIATE_DIGITS,
            BROWSE_MIDI,
            BROWSE_OUTPUT,
            CLOSE_PANEL,
            START_EXPORT,
            COUNTER_TEMPLATE_DEFAULT,
            COUNTER_TEMPLATE_FULL,
            COUNTER_BROWSE_FONT,
            COUNTER_BROWSE_CSV,
            COUNTER_RESET_PADDING,
            DC_BROWSE_FONT,
        ];
        let mut sorted = all.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "行 id 不允许重复");
    }
}
