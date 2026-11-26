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
    use lumino_ui_egui::chrome::VideoRowId;
    // Pick
    pub const CONTAINER: VideoRowId = VideoRowId::new(1);
    pub const CODEC: VideoRowId = VideoRowId::new(2);
    pub const BACKEND: VideoRowId = VideoRowId::new(3);
    pub const QUALITY: VideoRowId = VideoRowId::new(4);
    pub const RENDER_MODE: VideoRowId = VideoRowId::new(5);
    pub const MIDITRAIL_VIEW_MODE: VideoRowId = VideoRowId::new(7);
    pub const FPS: VideoRowId = VideoRowId::new(12);
    pub const MIDI_CONSOLE_BACKEND: VideoRowId = VideoRowId::new(13);
    pub const COUNTER_FONT_MODE: VideoRowId = VideoRowId::new(103);
    pub const COUNTER_FONT_FAMILY: VideoRowId = VideoRowId::new(104);
    pub const COUNTER_ALIGNMENT: VideoRowId = VideoRowId::new(107);
    pub const DC_METRIC: VideoRowId = VideoRowId::new(200);
    pub const DC_FONT_MODE: VideoRowId = VideoRowId::new(217);
    pub const DC_FONT_FAMILY: VideoRowId = VideoRowId::new(218);

    // Slider
    pub const WATERFALL_SPEED: VideoRowId = VideoRowId::new(6);
    pub const MIDITRAIL_Z_FAR: VideoRowId = VideoRowId::new(8);
    pub const COUNTER_FONT_SIZE: VideoRowId = VideoRowId::new(108);
    pub const DC_FONT_SIZE: VideoRowId = VideoRowId::new(216);

    // Toggle
    pub const MIDITRAIL_3D_NOTES: VideoRowId = VideoRowId::new(9);
    pub const COUNTER_USE_COMMAS: VideoRowId = VideoRowId::new(109);
    pub const COUNTER_PADDING_ZEROES: VideoRowId = VideoRowId::new(110);
    pub const COUNTER_SAVE_CSV: VideoRowId = VideoRowId::new(111);
    pub const DC_ABBREVIATE: VideoRowId = VideoRowId::new(234);
    pub const DC_SHOW_TEXT: VideoRowId = VideoRowId::new(235);
    pub const DC_SHOW_BARS: VideoRowId = VideoRowId::new(236);

    // Text
    pub const WIDTH: VideoRowId = VideoRowId::new(10);
    pub const HEIGHT: VideoRowId = VideoRowId::new(11);
    pub const OUTPUT_PATH: VideoRowId = VideoRowId::new(14);
    pub const COUNTER_TEXT: VideoRowId = VideoRowId::new(100);
    pub const COUNTER_FONT_PATH: VideoRowId = VideoRowId::new(105);
    pub const COUNTER_CSV_PATH: VideoRowId = VideoRowId::new(112);
    pub const COUNTER_CSV_FORMAT: VideoRowId = VideoRowId::new(114);
    pub const COUNTER_PAD_FIRST: VideoRowId = VideoRowId::new(120);
    pub const COUNTER_PAD_LAST: VideoRowId = VideoRowId::new(127);
    pub const DC_GRAPH_DURATION: VideoRowId = VideoRowId::new(201);
    pub const DC_ZOOM_SMOOTHNESS: VideoRowId = VideoRowId::new(202);
    pub const DC_GRAPH_SMOOTHNESS: VideoRowId = VideoRowId::new(203);
    pub const DC_PADDING_MUL: VideoRowId = VideoRowId::new(204);
    pub const DC_BG_COLOR: VideoRowId = VideoRowId::new(210);
    pub const DC_LINE_COLOR: VideoRowId = VideoRowId::new(211);
    pub const DC_TEXT_COLOR: VideoRowId = VideoRowId::new(212);
    pub const DC_BAR_COLOR: VideoRowId = VideoRowId::new(213);
    pub const DC_LINE_THICKNESS: VideoRowId = VideoRowId::new(214);
    pub const DC_BAR_THICKNESS: VideoRowId = VideoRowId::new(215);
    pub const DC_FONT_PATH: VideoRowId = VideoRowId::new(219);
    pub const DC_TEXT_X_OFFSET: VideoRowId = VideoRowId::new(230);
    pub const DC_TEXT_Y_OFFSET: VideoRowId = VideoRowId::new(231);
    pub const DC_MILESTONE_SCALE: VideoRowId = VideoRowId::new(232);
    pub const DC_ABBREVIATE_DIGITS: VideoRowId = VideoRowId::new(233);

    // Button
    pub const BROWSE_MIDI: VideoRowId = VideoRowId::new(15);
    pub const BROWSE_OUTPUT: VideoRowId = VideoRowId::new(16);
    pub const CLOSE_PANEL: VideoRowId = VideoRowId::new(17);
    pub const START_EXPORT: VideoRowId = VideoRowId::new(18);
    pub const COUNTER_TEMPLATE_DEFAULT: VideoRowId = VideoRowId::new(101);
    pub const COUNTER_TEMPLATE_FULL: VideoRowId = VideoRowId::new(102);
    pub const COUNTER_BROWSE_FONT: VideoRowId = VideoRowId::new(106);
    pub const COUNTER_BROWSE_CSV: VideoRowId = VideoRowId::new(113);
    pub const COUNTER_RESET_PADDING: VideoRowId = VideoRowId::new(128);
    pub const DC_BROWSE_FONT: VideoRowId = VideoRowId::new(220);
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
