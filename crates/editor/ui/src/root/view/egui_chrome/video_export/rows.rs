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
